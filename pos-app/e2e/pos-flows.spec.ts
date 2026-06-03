import { test, expect } from '@playwright/test';
import { PosPage } from './pages/PosPage';

test.describe.serial('POS Fiscal — E2E flows', () => {
  let pos: PosPage;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    pos = new PosPage(page);
    await page.goto('/');
    await pos.ensureSessionOpen();
  });

  // Flow 1 : ajouter un article → payer CB → TicketScreen → imprimer
  // Vérifie que [DEV PRINT] apparaît dans la console (printViaElectron en dev).
  test('Flow 1 — order → paiement CB → ticket → [DEV PRINT]', async () => {
    await pos.addItem('Burger Classic');
    await pos.submitCart();
    await pos.selectPaymentMethod('card');
    pos.clearConsoleLogs();
    await pos.confirmPayment();
    await expect(pos.page.getByText('Paiement accepté')).toBeVisible();
    await pos.clickPrint();
    expect(pos.consoleLogs.some((l) => l.includes('[DEV PRINT]'))).toBe(true);
    await pos.newOrder();
  });

  // Flow 2 : payer une commande → depuis TicketScreen, annuler avec motif
  // Vérifie que l'annulation NF525 est possible depuis le ticket.
  test('Flow 2 — paiement CB → ticket → annulation avec motif', async () => {
    await pos.addItem('Burger Classic');
    await pos.submitCart();
    await pos.selectPaymentMethod('card');
    await pos.confirmPayment();
    await expect(pos.page.getByText('Paiement accepté')).toBeVisible();
    await pos.navigateToCancel();
    await pos.selectCancelReason('Erreur de saisie');
    await pos.confirmCancel();
    await expect(pos.page.getByText('Caisse')).toBeVisible();
  });

  // Flow 3 : clôturer la session → rapport Z → imprimer
  // Doit être le dernier test — la session est fermée après cette opération.
  test('Flow 3 — rapport Z → clôturer → résultats → imprimer', async () => {
    await pos.navigateToZReport();
    await pos.closeSession();
    await expect(pos.page.getByText('Session clôturée')).toBeVisible();
    pos.clearConsoleLogs();
    await pos.clickPrint();
    expect(pos.consoleLogs.some((l) => l.includes('[DEV PRINT]'))).toBe(true);
  });
});
