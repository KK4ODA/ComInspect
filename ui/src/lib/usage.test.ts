import { describe, expect, it } from 'vitest';
import type { PortHolder } from './types';
import { describeHolder, holderName, holderNames, programName, span, usageSummary } from './usage';

function holder(p: Partial<PortHolder>): PortHolder {
  return { pid: 1, processName: 'x.exe', executable: null, description: null, ...p };
}

describe('port usage text', () => {
  it('names programs like the backend does', () => {
    expect(holderName(holder({ processName: 'VARAFM.exe', description: 'VARA FM' }))).toBe('VARA FM');
    expect(holderName(holder({ processName: 'VarAC.EXE', description: '  ' }))).toBe('VarAC');
    expect(describeHolder(holder({ pid: 4312, processName: 'VARAFM.exe', description: 'VARA FM' }))).toBe(
      'VARA FM (VARAFM.exe, PID 4312)',
    );
    expect(describeHolder(holder({ pid: 7, processName: 'flrig' }))).toBe('flrig (PID 7)');
    expect(describeHolder(holder({ pid: 7, processName: 'flrig', exiting: true }))).toBe(
      'flrig (PID 7), shutting down',
    );
  });

  it('joins several programs', () => {
    const list = ['fldigi', 'flrig', 'wsjtx'].map((n, i) => holder({ pid: i, processName: n }));
    expect(holderNames([])).toBe('');
    expect(holderNames(list.slice(0, 1))).toBe('fldigi');
    expect(holderNames(list.slice(0, 2))).toBe('fldigi and flrig');
    expect(holderNames(list)).toBe('fldigi, flrig and wsjtx');
  });

  it('shortens program paths', () => {
    expect(programName('C:\\VarAC\\VarAC.exe')).toBe('VarAC');
    expect(programName('C:\\Users\\ham\\Desktop\\VarAC.lnk')).toBe('VarAC');
    expect(programName('/Applications/fldigi.app')).toBe('fldigi');
    expect(programName('/usr/bin/js8call')).toBe('js8call');
  });

  it('formats how long a state has lasted', () => {
    expect(span(20_000)).toBe('less than a minute');
    expect(span(5 * 60_000)).toBe('5 min');
    expect(span(125 * 60_000)).toBe('2 h 5 min');
    expect(span(120 * 60_000)).toBe('2 h');
    expect(span(26 * 3600_000)).toBe('1 day');
    expect(span(-5)).toBe('less than a minute');
  });

  it('summarizes a port', () => {
    const base = { since: 0, sinceExact: true, watch: null };
    expect(usageSummary({ ...base, state: 'free' })).toBe('Not in use by any program');
    expect(usageSummary({ ...base, state: 'unknown', reason: 'the port does not exist' })).toBe(
      'Could not check which programs use this port (the port does not exist)',
    );
    expect(
      usageSummary({ ...base, state: 'in_use', holders: [holder({ pid: 9, processName: 'VarAC.exe' })] }),
    ).toBe('In use by VarAC (VarAC.exe, PID 9)');
  });
});
