// The fight HUD shared by every page that shows a Battle: name plates with HP
// bars, the clock, the countdown/result banner, the fight log and damage numbers.
// The page provides the #pa, #pb, #clock, #banner and #log elements.
import { Battle } from './battle';
import { Camera } from './render/camera';

export class FightHud {
  private banner = document.getElementById('banner') as HTMLDivElement;
  private logEl = document.querySelector('#log .sunken') as HTMLDivElement;
  private clock = document.getElementById('clock') as HTMLDivElement;
  private lastBanner = '';

  update(battle: Battle) {
    this.plate(battle, 'pa', 0);
    this.plate(battle, 'pb', 1);
    const t = battle.countdown;
    this.clock.textContent = t > 0 ? '' : String(Math.ceil(battle.timeLeft));
    let html = '', cls = 'window';
    if (battle.result) {
      const r = battle.result;
      html = `<div class="titlebar">Fight over</div><div class="wbody"><h1>${r.winner ? `${r.winner.name} wins` : 'Draw'}</h1>` +
        `<div class="why">by ${r.reason}</div></div>`;
    } else if (t > 0) {
      html = `<h1>${Math.ceil(t)}</h1>`;
      cls = 'window count';
    } else if (battle.countdown === 0 && battle.time < 3.3) {
      html = '<h1>CRAWL!</h1>';
      cls = 'window count';
    }
    if (html !== this.lastBanner) {
      this.lastBanner = html;
      this.banner.innerHTML = html;
      this.banner.className = cls;
      this.banner.style.display = html ? 'flex' : 'none';
    }
    this.logEl.innerHTML = battle.log.map((l, i) => (i === battle.log.length - 1 ? `<b>${l}</b>` : l)).join('\n');
  }

  private plate(battle: Battle, id: string, i: 0 | 1) {
    const f = battle.fighters[i];
    const el = document.getElementById(id)!;
    (el.querySelector('.name') as HTMLDivElement).textContent = f.name;
    (el.querySelector('.plan') as HTMLDivElement).textContent = f.out ? 'out of the ring' : f.hp <= 0 ? 'knocked out' : f.plan;
    (el.querySelector('.bar div') as HTMLDivElement).style.width = `${Math.max(0, f.hp)}%`;
  }

  /** Damage numbers float up from each hit. */
  drawPopups(g: CanvasRenderingContext2D, cam: Camera, battle: Battle, dpr: number) {
    g.font = `bold ${16 * dpr}px ui-monospace, Menlo, monospace`;
    g.textAlign = 'center';
    for (const p of battle.popups) {
      const [x, y] = cam.project(p.p);
      g.globalAlpha = Math.min(1, (1.1 - p.age) * 3);
      g.fillStyle = '#1d1918';
      g.fillText(p.text, x + 2 * dpr, y + 2 * dpr);
      g.fillStyle = p.hot ? '#ffd23a' : '#f2e6cf';
      g.fillText(p.text, x, y);
    }
    g.globalAlpha = 1;
  }
}
