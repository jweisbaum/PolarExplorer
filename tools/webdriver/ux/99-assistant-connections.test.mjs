/** Native Settings smoke test. It never installs a host plugin or uses an account. */
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { assert, newProject } from './harness.mjs';
let port;
export default {
  name: 'Native assistant status, permission choice and local connection test',
  async setup() {
    const profiles = await mkdtemp(join(tmpdir(), 'pe-assistant-profiles-'));
    const server = createServer();
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    port = server.address().port;
    await new Promise(resolve => server.close(resolve));
    return { env: { CODEX_HOME: join(profiles, 'codex'), CLAUDE_CONFIG_DIR: join(profiles, 'claude') }, teardown: () => rm(profiles, { recursive: true, force: true }) };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, 'Assistant settings');
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('[data-feature="settings:mcp"] strong', { text: 'Codex' });
    const section = '[data-feature="settings:mcp"]';
    assert.equal(await d.count(`${section} .settings-field strong`), 3);
    assert.equal(await d.run(`done(document.querySelector('${section} select').value)`), 'workflow');
    assert.equal(await d.run(`done(document.querySelector('${section} details').open)`), false);
    await d.run(`document.querySelector('${section}').scrollIntoView({block:'start'}); done(true);`);
    await t.shot('assistant-connections');
    await d.waitFor('[data-claude-desktop-setup]', { text: 'quit it completely' });
    await d.run(`document.querySelector('[data-claude-desktop-setup]').scrollIntoView({block:'center'}); done(true);`);
    await t.shot('windows-claude-setup');
    await d.click(`${section} summary`);
    await d.type('[data-feature="settings:mcp-port"] input', String(port));
    await d.key('Enter', '[data-feature="settings:mcp-port"] input');
    await d.click('[data-feature="settings:mcp-enable"] input');
    await d.waitFor(`${section} pre`, { text: String(port) });
    await d.click(`${section} button`, { text: 'Test' });
    await d.waitFor(section, { text: 'The app is reachable.' });
    await t.shot('local-test-passed');
    await d.click('[data-feature="settings:mcp-enable"] input');
    await d.waitGone(`${section} pre`);
  },
};
