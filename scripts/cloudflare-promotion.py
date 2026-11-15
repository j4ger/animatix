#!/usr/bin/env python3
"""Durable single-writer promotion journal prototype; deliberately no HTTP deploy adapter.

Use one persistent SQLite database, never a CI job-local copy. No expiring lease:
an uncertain request blocks all further promotions until positively reconciled.
All callers are trusted coordinator internals; this is not an authentication API.
"""
import json
import sqlite3
import uuid


class Rejected(ValueError):
    pass


class Coordinator:
    def __init__(self, path):
        self.db = sqlite3.connect(path, isolation_level=None, timeout=10)
        self.db.execute('PRAGMA synchronous=FULL')
        self.db.execute('CREATE TABLE IF NOT EXISTS journal (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)')
        self.db.execute('INSERT OR IGNORE INTO journal VALUES (1, ?)',
                        (json.dumps(dict(epoch=0, desired=None, active=None, pending=None)),))

    def close(self):
        self.db.close()

    def change(self, action):
        self.db.execute('BEGIN IMMEDIATE')
        try:
            state = json.loads(self.db.execute('SELECT body FROM journal WHERE id=1').fetchone()[0])
            result = action(state)
            self.db.execute('UPDATE journal SET body=? WHERE id=1', (json.dumps(state),))
            self.db.execute('COMMIT')
            return result
        except BaseException:
            self.db.execute('ROLLBACK')
            raise

    def select(self, expected_epoch, candidate):
        """CAS an explicit reviewed selection; build completion cannot select itself.

        candidate is a verified immutable receipt digest, not a run number or SHA order.
        Rollback selects a previously accepted receipt using a NEW epoch.
        """
        def action(state):
            if state['pending'] or type(expected_epoch) is not int or expected_epoch != state['epoch']:
                raise Rejected('stale selection or unresolved activation')
            if not isinstance(candidate, str) or len(candidate) != 64 or any(c not in '0123456789abcdef' for c in candidate):
                raise Rejected('expected verified receipt digest')
            state.update(epoch=expected_epoch+1, desired=candidate)
            return state['epoch']
        return self.change(action)

    def begin(self, epoch, candidate):
        """Commit intent BEFORE external POST. Exactly one caller receives a ticket."""
        def action(state):
            if state['pending'] or state['epoch'] != epoch or state['desired'] != candidate or epoch == 0:
                raise Rejected('stale completion or unresolved activation')
            ticket = str(uuid.uuid4())
            state['pending'] = dict(ticket=ticket, epoch=epoch, candidate=candidate)
            return ticket
        return self.change(action)

    def confirm(self, ticket, deployment_id):
        """Call only after terminal success verified against the pending version/receipt.

        A timeout or a GET showing the old version is NOT proof of non-execution.
        No retry/unlock API exists: uncertain POSTs stay blocked across restarts.
        """
        def action(state):
            pending = state['pending']
            if not pending or pending['ticket'] != ticket or not deployment_id:
                raise Rejected('wrong ticket or missing deployment evidence')
            state['active'] = dict(pending, deployment_id=deployment_id)
            state['pending'] = None
            # Consume selection: the same completed candidate cannot re-activate.
            state['desired'] = None
        return self.change(action)
