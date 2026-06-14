// Minimal DOM helpers so views stay terse without pulling in a framework.

type Child = Node | string | null | undefined | false;

export interface ElProps {
  class?: string;
  id?: string;
  type?: string;
  value?: string;
  placeholder?: string;
  textContent?: string;
  title?: string;
  href?: string;
  min?: string;
  max?: string;
  step?: string;
  disabled?: boolean;
  checked?: boolean;
  autofocus?: boolean;
  rows?: number;
  onClick?: (ev: MouseEvent) => void;
  onInput?: (ev: Event) => void;
  onChange?: (ev: Event) => void;
  onSubmit?: (ev: SubmitEvent) => void;
  onKeydown?: (ev: KeyboardEvent) => void;
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: ElProps = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);

  if (props.class !== undefined) node.className = props.class;
  if (props.id !== undefined) node.id = props.id;
  if (props.textContent !== undefined) node.textContent = props.textContent;
  if (props.title !== undefined) node.title = props.title;

  if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
    if (props.value !== undefined) node.value = props.value;
    if (props.placeholder !== undefined) node.placeholder = props.placeholder;
    if (props.disabled !== undefined) node.disabled = props.disabled;
  }
  if (node instanceof HTMLInputElement) {
    if (props.type !== undefined) node.type = props.type;
    if (props.min !== undefined) node.min = props.min;
    if (props.max !== undefined) node.max = props.max;
    if (props.step !== undefined) node.step = props.step;
    if (props.checked !== undefined) node.checked = props.checked;
  }
  if (node instanceof HTMLTextAreaElement && props.rows !== undefined) {
    node.rows = props.rows;
  }
  if (node instanceof HTMLButtonElement) {
    if (props.disabled !== undefined) node.disabled = props.disabled;
    // Default buttons to type="button" so they never accidentally submit a
    // surrounding <form>; views opt in to "submit" explicitly.
    node.type = props.type === "submit" ? "submit" : "button";
  }
  if (node instanceof HTMLAnchorElement && props.href !== undefined) {
    node.href = props.href;
  }

  if (props.onClick) node.addEventListener("click", props.onClick as EventListener);
  if (props.onInput) node.addEventListener("input", props.onInput);
  if (props.onChange) node.addEventListener("change", props.onChange);
  if (props.onSubmit) node.addEventListener("submit", props.onSubmit as EventListener);
  if (props.onKeydown) node.addEventListener("keydown", props.onKeydown as EventListener);

  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child);
  }

  if (props.autofocus) {
    // Defer so the element is in the document before focusing.
    queueMicrotask(() => {
      if (node instanceof HTMLElement) node.focus();
    });
  }

  return node;
}

/** Remove all children from a node. */
export function clear(node: HTMLElement): void {
  node.replaceChildren();
}

/** Append children, skipping null/undefined/false (mirrors `el`'s children). */
export function append(node: HTMLElement, ...children: Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child);
  }
}
