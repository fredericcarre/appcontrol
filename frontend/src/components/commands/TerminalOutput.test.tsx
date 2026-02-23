import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TerminalOutput } from './TerminalOutput';

describe('TerminalOutput', () => {
  it('should render stdout lines with green color class', () => {
    render(
      <TerminalOutput
        lines={[{ text: 'Hello stdout', type: 'stdout' }]}
      />,
    );

    expect(screen.getByText('Hello stdout')).toBeInTheDocument();
    expect(screen.getByText('Hello stdout').className).toContain('text-green-300');
  });

  it('should render stderr lines with red color class', () => {
    render(
      <TerminalOutput
        lines={[{ text: 'Error output', type: 'stderr' }]}
      />,
    );

    expect(screen.getByText('Error output')).toBeInTheDocument();
    expect(screen.getByText('Error output').className).toContain('text-red-400');
  });

  it('should render info lines with blue color class', () => {
    render(
      <TerminalOutput
        lines={[{ text: 'Info message', type: 'info' }]}
      />,
    );

    expect(screen.getByText('Info message')).toBeInTheDocument();
    expect(screen.getByText('Info message').className).toContain('text-blue-400');
  });

  it('should render lines without type as stdout (green)', () => {
    render(
      <TerminalOutput
        lines={[{ text: 'Default output' }]}
      />,
    );

    expect(screen.getByText('Default output')).toBeInTheDocument();
    expect(screen.getByText('Default output').className).toContain('text-green-300');
  });

  it('should render multiple lines', () => {
    render(
      <TerminalOutput
        lines={[
          { text: 'Line 1', type: 'stdout' },
          { text: 'Line 2', type: 'stderr' },
          { text: 'Line 3', type: 'info' },
        ]}
      />,
    );

    expect(screen.getByText('Line 1')).toBeInTheDocument();
    expect(screen.getByText('Line 2')).toBeInTheDocument();
    expect(screen.getByText('Line 3')).toBeInTheDocument();
  });

  it('should render empty when no lines', () => {
    const { container } = render(<TerminalOutput lines={[]} />);
    const terminalDiv = container.firstChild as HTMLElement;
    // No line divs inside
    expect(terminalDiv.children).toHaveLength(0);
  });

  it('should have terminal styling classes', () => {
    const { container } = render(<TerminalOutput lines={[]} />);
    const terminalDiv = container.firstChild as HTMLElement;
    expect(terminalDiv.className).toContain('bg-gray-950');
    expect(terminalDiv.className).toContain('font-mono');
    expect(terminalDiv.className).toContain('text-xs');
  });

  it('should pass through additional class names', () => {
    const { container } = render(
      <TerminalOutput lines={[]} className="custom-class" />,
    );
    const terminalDiv = container.firstChild as HTMLElement;
    expect(terminalDiv.className).toContain('custom-class');
  });

  it('should render whitespace-pre-wrap for preserving whitespace', () => {
    render(
      <TerminalOutput
        lines={[{ text: '  indented output  ', type: 'stdout' }]}
      />,
    );

    const lineDiv = screen.getByText('indented output').closest('div');
    expect(lineDiv?.className).toContain('whitespace-pre-wrap');
  });

  it('should have displayName set to TerminalOutput', () => {
    expect(TerminalOutput.displayName).toBe('TerminalOutput');
  });
});
