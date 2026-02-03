import * as vscode from 'vscode';
import { ComponentEvent, LogMessage } from './models/common';

export type ObservabilityItem = Record<string, unknown> | string;

// ANSI color codes
const COLORS = {
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  gray: '\x1b[90m',
  reset: '\x1b[0m',
  bold: '\x1b[1m',
};

export class ObservabilityViewer {
  private output: vscode.OutputChannel;

  constructor(title: string) {
    this.output = vscode.window.createOutputChannel(title);
  }

  show() {
    this.output.show();
  }

  appendHeader(title: string) {
    this.output.appendLine(`\n=== ${title} ===`);
  }

  appendItems(items: ObservabilityItem[]) {
    for (const item of items) {
      if (typeof item === 'string') {
        this.output.appendLine(item);
      } else {
        this.output.appendLine(JSON.stringify(item, null, 2));
      }
    }
  }

  appendLogMessages(logs: LogMessage[]) {
    for (const log of logs) {
      const ts = log.timestamp ? new Date(log.timestamp).toISOString() : '';
      const level = (log.level || 'INFO').toUpperCase().padEnd(5);
      this.output.appendLine(`[${ts}] ${level} ${log.message}`);
    }
  }

  appendRaw(raw: string) {
    this.output.appendLine(raw);
  }

  appendError(message: string) {
    this.output.appendLine(`ERROR: ${message}`);
  }

  dispose() {
    this.output.dispose();
  }
}

class LogTerminalPty implements vscode.Pseudoterminal {
  private writeEmitter = new vscode.EventEmitter<string>();
  onDidWrite: vscode.Event<string> = this.writeEmitter.event;

  private closeEmitter = new vscode.EventEmitter<void>();
  onDidClose: vscode.Event<void> = this.closeEmitter.event;

  private ready = false;
  private pendingWrites: string[] = [];

  open(): void {
    this.ready = true;
    for (const text of this.pendingWrites) {
      this.writeEmitter.fire(text);
    }
    this.pendingWrites = [];
  }

  close(): void {
    this.ready = false;
  }

  writeLine(text: string): void {
    const output = text + '\r\n';
    if (this.ready) {
      this.writeEmitter.fire(output);
    } else {
      this.pendingWrites.push(output);
    }
  }
}

export class LogTerminalViewer {
  private terminal: vscode.Terminal;
  private pty: LogTerminalPty;

  constructor(title: string) {
    this.pty = new LogTerminalPty();
    this.terminal = vscode.window.createTerminal({ name: title, pty: this.pty });
  }

  show() {
    this.terminal.show();
  }

  appendHeader(title: string) {
    this.pty.writeLine(`\n${COLORS.bold}=== ${title} ===${COLORS.reset}`);
  }

  appendLogMessages(logs: LogMessage[]) {
    for (const log of logs) {
      const ts = log.timestamp ? new Date(log.timestamp).toISOString() : '';
      const level = (log.level || 'INFO').toUpperCase().padEnd(5);
      const color = this.getLevelColor(log.level);
      this.pty.writeLine(`${COLORS.gray}[${ts}]${COLORS.reset} ${color}${level}${COLORS.reset} ${log.message}`);
    }
  }

  appendLogMessage(log: LogMessage) {
    const ts = log.timestamp ? new Date(log.timestamp).toISOString() : '';
    const level = (log.level || 'INFO').toUpperCase().padEnd(5);
    const color = this.getLevelColor(log.level);
    this.pty.writeLine(`${COLORS.gray}[${ts}]${COLORS.reset} ${color}${level}${COLORS.reset} ${log.message}`);
  }

  private getLevelColor(level: string | undefined): string {
    switch (level?.toLowerCase()) {
      case 'error':
        return COLORS.red;
      case 'warn':
        return COLORS.yellow;
      case 'debug':
        return COLORS.gray;
      case 'trace':
        return COLORS.gray;
      default:
        return COLORS.cyan;
    }
  }

  appendEvents(events: ComponentEvent[]) {
    for (const event of events) {
      this.appendEvent(event);
    }
  }

  appendEvent(event: ComponentEvent) {
    const ts = event.timestamp ? new Date(event.timestamp).toISOString() : '';
    const status = (event.status || 'Unknown').padEnd(12);
    const color = this.getStatusColor(event.status);
    const message = event.message || '';
    this.pty.writeLine(`${COLORS.gray}[${ts}]${COLORS.reset} ${color}${status}${COLORS.reset} ${message}`);
  }

  private getStatusColor(status: string | undefined): string {
    switch (status) {
      case 'Running':
        return COLORS.green;
      case 'Error':
      case 'Failed':
      case 'TerminalError':
        return COLORS.red;
      case 'Starting':
      case 'Stopping':
        return COLORS.yellow;
      case 'Stopped':
        return COLORS.gray;
      default:
        return COLORS.cyan;
    }
  }

  appendRaw(raw: string) {
    this.pty.writeLine(raw);
  }

  appendError(message: string) {
    this.pty.writeLine(`${COLORS.red}ERROR: ${message}${COLORS.reset}`);
  }
}
