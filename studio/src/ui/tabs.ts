/**
 * Mobile tab navigation: below 900px the three workbench panes are
 * switched with the tab bar instead of shown side by side.
 */

export class Tabs {
  private tabbar: HTMLElement;

  constructor(tabbar: HTMLElement, private onSwitch: (tab: string) => void) {
    this.tabbar = tabbar;
    for (const btn of Array.from(this.tabbar.querySelectorAll('.tab'))) {
      btn.addEventListener('click', () => this.select((btn as HTMLElement).dataset.tab!));
    }
    this.select('editor');
  }

  select(tab: string): void {
    for (const btn of Array.from(this.tabbar.querySelectorAll('.tab'))) {
      btn.classList.toggle('active', (btn as HTMLElement).dataset.tab === tab);
    }
    this.onSwitch(tab);
  }
}
