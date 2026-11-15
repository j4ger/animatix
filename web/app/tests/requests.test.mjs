import test from 'node:test';
import assert from 'node:assert/strict';
import {withTimeout} from '../src/lib/requests.ts';
test('bounded wait keeps successful results',async()=>assert.equal(await withTimeout(Promise.resolve(42),100,'timeout'),42));
test('bounded wait reports stalled setup',async()=>await assert.rejects(withTimeout(new Promise(()=>{}),5,'Please retry'),/Please retry/));
test('bounded wait retains the original error',async()=>await assert.rejects(withTimeout(Promise.reject(new Error('bad module')),100,'timeout'),/bad module/));
