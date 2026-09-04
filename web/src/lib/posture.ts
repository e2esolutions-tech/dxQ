// Posture-band classifiers — keep these in lockstep with the paper's
// thresholds and with ree0xq-server's tailwind palette aliases.

export type Band = "good" | "plan" | "urgent" | "critical";

export function qBand(q: number): Band {
  if (q < 0.30) return "good";
  if (q < 0.60) return "plan";
  if (q < 0.80) return "urgent";
  return "critical";
}

export function qColorClass(q: number): string {
  switch (qBand(q)) {
    case "good":
      return "text-posture-good";
    case "plan":
      return "text-posture-plan";
    case "urgent":
      return "text-posture-urgent";
    case "critical":
      return "text-posture-critical";
  }
}

export function qBgClass(q: number): string {
  switch (qBand(q)) {
    case "good":
      return "bg-posture-good/15 text-posture-good";
    case "plan":
      return "bg-posture-plan/15 text-posture-plan";
    case "urgent":
      return "bg-posture-urgent/15 text-posture-urgent";
    case "critical":
      return "bg-posture-critical/20 text-posture-critical";
  }
}

export function formatQ(q: number): string {
  return q.toFixed(2);
}

export function daysUntil(iso: string): number {
  const now = Date.now();
  const d = new Date(iso).getTime();
  return Math.max(0, Math.round((d - now) / (1000 * 60 * 60 * 24)));
}

export function deadlineLabel(iso: string): string {
  const days = daysUntil(iso);
  if (days === 0) return "deadline reached";
  if (days < 30) return `${days} days remaining`;
  if (days < 365) return `${Math.round(days / 30)} months remaining`;
  return `${(days / 365).toFixed(1)} years remaining`;
}
