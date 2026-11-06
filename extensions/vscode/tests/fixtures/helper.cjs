let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', value => {
  input += value;
  if (!input.includes('\n')) return;
  // Node normally evaluates stdin as JavaScript; this fixture acts as a native helper instead.
  process.stdin.removeAllListeners('end');
  process.stdin.removeAllListeners('data');
  const request = JSON.parse(input);
  if (request.operationId === 'malformed') {
    process.stdout.write('{"type":"result","activeProviderId":null}\n');
  } else if (request.action === 'switch') {
    process.stdout.write(JSON.stringify({ type: 'error', message: `failed ${request.apiKey} ${request.journalKey}` }) + '\n');
  } else {
    process.stdout.write(JSON.stringify({ type: 'progress', stage: 'test', detail: `working ${request.apiKey}` }) + '\n');
    process.stdout.write(JSON.stringify({ type: 'result', protocolVersion: 1, activeProviderId: null, officialModel: request.officialModel, hasKey: !!request.apiKey, childArgs: process.argv.slice(1) }) + '\n');
  }
  process.stdout.write('', () => process.exit(0));
});
