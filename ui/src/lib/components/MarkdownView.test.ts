import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import MarkdownView from './MarkdownView.svelte';

describe('MarkdownView', () => {
  it('pinta los nodos del Markdown sin crear HTML a partir del texto', () => {
    const { container } = render(MarkdownView, {
      props: {
        source: '## Título\n\n<script>window.__pwn = 1</script> y <b>texto</b>\n\n- uno\n- [x] hecho\n\n```\n<img src=x onerror=alert(1)>\n```'
      }
    });
    expect(screen.getByRole('heading', { level: 3, name: 'Título' })).toBeInTheDocument();
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('b')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent).toContain('<script>window.__pwn = 1</script>');
    expect(container.textContent).toContain('<img src=x onerror=alert(1)>');
    expect((window as unknown as { __pwn?: number }).__pwn).toBeUndefined();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByRole('checkbox', { name: 'Tarea hecha' })).toBeChecked();
  });

  it('solo los enlaces https son enlaces, y se abren por la aplicación', async () => {
    const onopen = vi.fn();
    render(MarkdownView, {
      props: { source: '[bien](https://example.com/ok) [mal](javascript:alert(1)) [http](http://example.com)', onopen }
    });
    const links = screen.getAllByRole('link');
    expect(links).toHaveLength(1);
    expect(links[0]).toHaveAttribute('href', 'https://example.com/ok');
    expect(links[0]).toHaveAttribute('rel', 'noopener noreferrer');
    await userEvent.click(links[0] as HTMLElement);
    expect(onopen).toHaveBeenCalledExactlyOnceWith('https://example.com/ok');
  });

  it('sin contenido muestra el texto vacío indicado', () => {
    render(MarkdownView, { props: { source: '  ', empty: 'Sin descripción.' } });
    expect(screen.getByText('Sin descripción.')).toBeInTheDocument();
  });
});
