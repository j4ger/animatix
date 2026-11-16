#!/usr/bin/env python3
"""Single-host preview release coordinator with durable intent and no POST retries.

No HTTP listener or network-facing administration API. The private OS directory
is the authorization boundary. Production names are unconditionally rejected.
"""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import stat
import sys
import uuid

spec = importlib.util.spec_from_file_location('control', Path(__file__).with_name('cloudflare-control.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


class Blocked(RuntimeError):
    pass


class Store:
    def __init__(self, directory, config):
        directory = Path(directory)
        info = directory.lstat()
        if not directory.is_absolute() or not stat.S_ISDIR(info.st_mode) \
                or stat.S_IMODE(info.st_mode) != 0o700 or info.st_uid != os.getuid():
            raise Blocked('state directory must be an existing private, owned absolute directory (0700)')
        self.lock = os.open(directory/'coordinator.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        # Non-expiring single-host lock held through remote calls and journal writes.
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BaseException:
            os.close(self.lock)
            raise
        db_path = directory/'coordinator.sqlite'
        if db_path.is_symlink():
            self.close_lock()
            raise Blocked('linked journal forbidden')
        self.db = sqlite3.connect(db_path, isolation_level=None)
        self.db.execute('PRAGMA synchronous=FULL')
        self.db.executescript('''
          CREATE TABLE IF NOT EXISTS state (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS candidates (digest TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, body TEXT NOT NULL);
        ''')
        self.config = config
        self.config_digest = hashlib.sha256(c.canonical(config)).hexdigest()

    def close_lock(self):
        fcntl.flock(self.lock, fcntl.LOCK_UN)
        os.close(self.lock)

    def close(self):
        self.db.close()
        self.close_lock()

    def state(self):
        row = self.db.execute('SELECT body FROM state WHERE id=1').fetchone()
        if not row:
            raise Blocked('journal uninitialized; explicit first-time initialization required')
        value = json.loads(row[0])
        if value['config_digest'] != self.config_digest:
            raise Blocked('target configuration changed; do not reuse or reset journal')
        return value

    def save(self, state, event):
        self.db.execute('BEGIN IMMEDIATE')
        try:
            self.db.execute('INSERT OR REPLACE INTO state VALUES (1, ?)', (json.dumps(state),))
            self.db.execute('INSERT INTO events (body) VALUES (?)', (json.dumps(event),))
            self.db.execute('COMMIT')
        except BaseException:
            self.db.execute('ROLLBACK')
            raise

    def register(self, digest, evidence):
        self.state()
        if hashlib.sha256(c.canonical(evidence)).hexdigest() != digest:
            raise Blocked('admission digest mismatch')
        self.db.execute('INSERT OR IGNORE INTO candidates VALUES (?, ?)', (digest, json.dumps(evidence)))

    def candidate(self, digest):
        row = self.db.execute('SELECT body FROM candidates WHERE digest=?', (digest,)).fetchone()
        if not row:
            raise Blocked('candidate has not passed authenticated admission')
        return json.loads(row[0])


class Coordinator:
    def __init__(self, store, api, enabled=False):
        self.store, self.api, self.enabled = store, api, enabled
        self.path = '/workers/scripts/'+store.config['worker']+'/deployments'

    def latest(self):
        deployments = self.api('GET', self.path)['deployments']
        if not isinstance(deployments, list):
            raise Blocked('invalid deployment inventory')
        if not deployments:
            return None
        # API returns deployments newest-first. Fetch exact ID again before use.
        head = deployments[0]
        c.identifier(head['id'])
        return head

    def initialize(self, acknowledged=False):
        if not acknowledged or self.store.db.execute('SELECT 1 FROM state').fetchone():
            raise Blocked('only an explicitly acknowledged NEW single-writer installation may initialize')
        baseline = self.latest()
        state = dict(config_digest=self.store.config_digest, epoch=0, desired=None, pending=None,
                     deployment_id=None if baseline is None else baseline['id'], active_candidate=None)
        self.store.save(state, {'event': 'initialize', 'baseline': baseline})

    def select(self, expected_epoch, digest):
        state = self.store.state()
        self.store.candidate(digest)
        if type(expected_epoch) is not int or state['epoch'] != expected_epoch or state['pending']:
            raise Blocked('stale epoch or unresolved activation')
        state.update(epoch=expected_epoch+1, desired=digest)
        self.store.save(state, {'event': 'select', 'epoch': state['epoch'], 'candidate': digest})
        return state['epoch']

    def preflight(self, state, candidate):
        if (self.latest() or {}).get('id') != state['deployment_id']:
            raise Blocked('external deployment drift; sole-writer invariant violated')
        c.load('cloudflare-candidate-admission').version_check(self.api, self.store.config, candidate['version_id'])

    def activate(self, epoch, digest):
        if not self.enabled:
            raise Blocked('preview activation disabled')
        state = self.store.state()
        if type(epoch) is not int or state['epoch'] != epoch or state['desired'] != digest or state['pending']:
            raise Blocked('stale completion, consumed selection or unresolved activation')
        candidate = self.store.candidate(digest)
        self.preflight(state, candidate)
        ticket = str(uuid.uuid4())
        pending = dict(ticket=ticket, epoch=epoch, candidate=digest, version_id=candidate['version_id'], phase='prepared')
        state['pending'] = pending
        self.store.save(state, {'event': 'prepare', **pending})
        # Persist sent BEFORE any possible POST. A crash in the next two lines
        # intentionally sacrifices liveness; it must never permit a duplicate POST.
        pending['phase'] = 'sent'
        self.store.save(state, {'event': 'send-intent', 'ticket': ticket})
        payload = {'strategy': 'percentage', 'versions': [{'version_id': pending['version_id'], 'percentage': 100}],
                   'annotations': {'workers/message': 'animatix-ticket:'+ticket}}
        response = self.api('POST', self.path, payload)
        # Even HTTP success is insufficient if it does not identify this exact ticket.
        self.check_deployment(response, pending)
        return self.reconcile()

    def check_deployment(self, deployment, pending):
        if not isinstance(deployment, dict):
            raise Blocked('no positive deployment outcome observed')
        c.identifier(deployment['id'])
        if deployment.get('strategy') != 'percentage' or deployment.get('versions') != [
                {'version_id': pending['version_id'], 'percentage': 100}] \
                or deployment.get('annotations', {}).get('workers/message') != 'animatix-ticket:'+pending['ticket']:
            raise Blocked('deployment response does not match durable activation intent')

    def reconcile(self):
        state = self.store.state()
        pending = state['pending']
        if not pending:
            raise Blocked('no pending activation')
        if pending['phase'] == 'prepared':
            # Holding the OS lock proves no previous process can advance this intent.
            # Code never sends a request before recording phase=sent.
            state.update(pending=None, desired=None)
            self.store.save(state, {'event': 'abandon-unsent', 'ticket': pending['ticket']})
            return 'unsent intent abandoned; select again with a new epoch'
        head = self.latest()
        self.check_deployment(head, pending)
        detail = self.api('GET', self.path+'/'+head['id'])
        self.check_deployment(detail, pending)
        if detail['id'] != head['id'] or self.latest()['id'] != head['id']:
            raise Blocked('deployment changed during reconciliation')
        state.update(pending=None, desired=None, deployment_id=head['id'], active_candidate=pending['candidate'])
        self.store.save(state, {'event': 'confirmed', 'ticket': pending['ticket'], 'deployment': detail})
        return head['id']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state-directory', required=True, type=Path)
    sub = parser.add_subparsers(dest='command', required=True)
    init = sub.add_parser('init')
    init.add_argument('--new-single-writer-installation', action='store_true')
    sub.add_parser('status')
    admit = sub.add_parser('admit')
    admit.add_argument('--build', required=True)
    admit.add_argument('--gh', required=True, type=Path)
    select = sub.add_parser('select')
    select.add_argument('--expected-epoch', required=True, type=int)
    select.add_argument('--candidate', required=True)
    select.add_argument('--reviewed-browser-and-receipt', action='store_true')
    activate = sub.add_parser('activate')
    activate.add_argument('--epoch', required=True, type=int)
    activate.add_argument('--candidate', required=True)
    sub.add_parser('reconcile')
    args = parser.parse_args()
    config = c.target()
    store = Store(args.state_directory, config)
    try:
        if args.command == 'status':
            print(json.dumps(store.state(), sort_keys=True))
            return
        api = c.API(config, os.environ.get('CLOUDFLARE_PREVIEW_COORDINATOR_TOKEN'))
        coordinator = Coordinator(store, api, os.environ.get('ANIMATIX_CF_PREVIEW_ACTIVATION') == 'enabled')
        if args.command == 'init':
            coordinator.initialize(args.new_single_writer_installation)
        elif args.command == 'admit':
            digest, evidence = c.load('cloudflare-candidate-admission').admit(api, config, args.build, args.gh)
            store.register(digest, evidence)
            print('CANDIDATE_ADMITTED', digest)
        elif args.command == 'select':
            if not args.reviewed_browser_and_receipt:
                raise Blocked('explicit browser/receipt review required before selection')
            print('SELECTED_EPOCH', coordinator.select(args.expected_epoch, args.candidate))
        elif args.command == 'activate':
            print('PREVIEW_ACTIVATED', coordinator.activate(args.epoch, args.candidate))
        elif args.command == 'reconcile':
            print('RECONCILED', coordinator.reconcile())
    finally:
        store.close()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print('COORDINATOR_BLOCKED', str(error) if isinstance(error, (Blocked, ValueError, c.ApiError)) else type(error).__name__, file=sys.stderr)
        raise SystemExit(1)
