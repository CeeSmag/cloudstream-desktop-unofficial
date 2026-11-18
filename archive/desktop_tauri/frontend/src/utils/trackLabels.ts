/**
 * Makes menu labels unique: when several entries share a name (nine "English" subtitle files),
 * the second and later ones get a number so the user can tell them apart.
 */
export function numberDuplicateLabels(labels: string[]): string[] {
  const total = new Map<string, number>();
  for (const label of labels) total.set(label, (total.get(label) ?? 0) + 1);

  const seen = new Map<string, number>();
  return labels.map((label) => {
    if ((total.get(label) ?? 0) < 2) return label;
    const position = (seen.get(label) ?? 0) + 1;
    seen.set(label, position);
    return `${label} ${position}`;
  });
}
