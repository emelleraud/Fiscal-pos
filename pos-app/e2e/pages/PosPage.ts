import type { Page, ConsoleMessage } from '@playwright/test';

type PaymentMethod = 'card' | 'cash' | 'meal_voucher';

const PAYMENT_LABELS: Record<PaymentMethod, string> = {
  card: 'Carte bancaire',
  cash: 'Espèces',
  meal_voucher: 'Ticket restaurant',
};

export class PosPage {
  private _consoleLogs: string[] = [];

  constructor(readonly page: Page) {
    page.on('console', (msg: ConsoleMessage) => {
      this._consoleLogs.push(msg.text());
    });
  }

  get consoleLogs(): string[] {
    return this._consoleLogs;
  }

  clearConsoleLogs(): void {
    this._consoleLogs = [];
  }

  async ensureSessionOpen(): Promise<void> {
    await this.page.waitForLoadState('networkidle');
    const openBtn = this.page.getByRole('button', { name: 'Ouvrir la caisse' });
    if (await openBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await openBtn.click();
    }
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 10_000 });
  }

  async addItem(name: string): Promise<void> {
    await this.page.getByText(name).first().click();
  }

  async submitCart(): Promise<void> {
    await this.page.getByRole('button', { name: /Encaisser/ }).click();
    await this.page.getByText('À encaisser').waitFor({ state: 'visible', timeout: 10_000 });
  }

  async selectPaymentMethod(method: PaymentMethod): Promise<void> {
    await this.page.getByText(PAYMENT_LABELS[method]).first().click();
  }

  async confirmPayment(): Promise<void> {
    await this.page.getByRole('button', { name: /Paiement reçu/ }).click();
    await this.page.getByText('Paiement accepté').waitFor({ state: 'visible', timeout: 10_000 });
  }

  // Clique "Imprimer" ou "Imprimer rapport Z" et attend le log [DEV PRINT].
  async clickPrint(): Promise<void> {
    const devPrintPromise = this.page.waitForEvent('console', {
      predicate: (msg) => msg.text().includes('[DEV PRINT]'),
      timeout: 5_000,
    });
    await this.page.getByRole('button', { name: /Imprimer/ }).click();
    await devPrintPromise;
  }

  async newOrder(): Promise<void> {
    await this.page.getByRole('button', { name: 'Nouvelle commande' }).click();
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 5_000 });
  }

  async navigateToCancel(): Promise<void> {
    await this.page.getByRole('button', { name: 'Annulation' }).click();
    await this.page.getByText('Annulation de commande').waitFor({ state: 'visible', timeout: 5_000 });
  }

  async selectCancelReason(reason: string): Promise<void> {
    await this.page.getByRole('button', { name: reason }).click();
  }

  async confirmCancel(): Promise<void> {
    await this.page.getByRole('checkbox').check();
    await this.page.getByRole('button', { name: "Confirmer l'annulation" }).click();
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 10_000 });
  }

  async navigateToZReport(): Promise<void> {
    await this.page.getByRole('button', { name: 'Rapport Z' }).click();
    await this.page.getByRole('heading', { name: 'Rapport Z' }).waitFor({ state: 'visible', timeout: 5_000 });
  }

  async closeSession(): Promise<void> {
    await this.page.getByRole('button', { name: /Clôturer/ }).click();
    await this.page.getByText('Session clôturée').waitFor({ state: 'visible', timeout: 15_000 });
  }
}
