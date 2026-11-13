const T_975 = [
  null,
  12.706,
  4.303,
  3.182,
  2.776,
  2.571,
  2.447,
  2.365,
  2.306,
  2.262,
  2.228,
  2.201,
  2.179,
  2.160,
  2.145,
  2.131,
  2.120,
  2.110,
  2.101,
  2.093,
  2.086,
  2.080,
  2.074,
  2.069,
  2.064,
  2.060,
  2.056,
  2.052,
  2.048,
  2.045,
  2.042,
];

function tCritical(df) {
  if (df < 1) return Infinity;
  if (df < T_975.length) return T_975[df];
  if (df <= 40) return 2.021;
  if (df <= 60) return 2.000;
  if (df <= 120) return 1.980;
  return 1.960;
}

function summarize(values) {
  const n = values.length;
  const mean = values.reduce((sum, value) => sum + value, 0) / n;
  const variance = n > 1
    ? values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / (n - 1)
    : Infinity;
  return { n, mean, sd: Math.sqrt(variance) };
}

function interval(values) {
  if (values.length === 0) return { estimate: null, ci95: [null, null], method: 'unverified' };
  const { n, mean, sd } = summarize(values);
  if (n < 2) return { estimate: mean, ci95: [null, null], method: 'paired two-sided 95% Student-t; fewer than two pairs' };
  const halfWidth = tCritical(n - 1) * sd / Math.sqrt(n);
  return { estimate: mean, ci95: [mean - halfWidth, mean + halfWidth], method: 'paired two-sided 95% Student-t' };
}

/**
 * Compare paired costs. A positive delta always means the new version costs
 * more; for higher-is-better metrics that means old - new (for example FPS
 * lost). Ratios use the same direction. Partial pairs stay visible and cannot
 * produce a passing required result.
 */
export function comparePaired(oldValues, newValues, {
  threshold = 1.05,
  absoluteThreshold = 0,
  higherIsBetter = false,
  required = false,
  minimumSamples = 3,
} = {}) {
  if (oldValues.length !== newValues.length) throw new RangeError('paired samples must have equal lengths');
  if (!Number.isFinite(threshold) || threshold < 1) throw new RangeError('relative threshold must be finite and at least 1');
  if (!Number.isFinite(absoluteThreshold) || absoluteThreshold < 0) throw new RangeError('absolute threshold must be finite and non-negative');
  if (!Number.isInteger(minimumSamples) || minimumSamples < 3) throw new RangeError('minimum valid samples must be an integer of at least 3');

  const ratios = [];
  const deltas = [];
  const skipped = [];
  for (let i = 0; i < oldValues.length; i += 1) {
    const oldValue = oldValues[i];
    const newValue = newValues[i];
    if (!Number.isFinite(oldValue) || !Number.isFinite(newValue) || oldValue <= 0 || newValue <= 0) {
      skipped.push(i);
      continue;
    }
    ratios.push(higherIsBetter ? oldValue / newValue : newValue / oldValue);
    deltas.push(higherIsBetter ? oldValue - newValue : newValue - oldValue);
  }

  const expectedN = oldValues.length;
  const coverage = { n: ratios.length, expected_n: expectedN, skipped_indices: skipped };
  if (ratios.length === 0) {
    return {
      status: required ? 'inconclusive' : 'unverified',
      reason: 'no valid paired samples',
      ...coverage,
      minimum_samples: minimumSamples,
      threshold,
      absolute_threshold: absoluteThreshold,
      direction: 'higher-is-worse',
      ratios,
      deltas,
      ratio_ci95: [null, null],
      delta_ci95: [null, null],
    };
  }

  const logRatioInterval = interval(ratios.map(Math.log));
  const deltaInterval = interval(deltas);
  const ratioCi = logRatioInterval.ci95.map(value => value === null ? null : Math.exp(value));
  const complete = skipped.length === 0;
  let status;
  let reason;
  if (ratios.length < minimumSamples) {
    status = required ? 'inconclusive' : 'unverified';
    reason = `only ${ratios.length} valid pairs; at least ${minimumSamples} are required for a 95% interval`;
  } else if (!complete) {
    status = required ? 'inconclusive' : 'incomplete';
    reason = 'one or more paired samples were invalid';
  } else if (!ratioCi.every(Number.isFinite) || !deltaInterval.ci95.every(Number.isFinite)) {
    status = required ? 'inconclusive' : 'unverified';
    reason = 'confidence interval is not finite';
  } else if (ratioCi[0] > threshold && deltaInterval.ci95[0] > absoluteThreshold) {
    status = 'regression';
    reason = 'both relative and absolute impact floors cleared by the lower 95% bounds';
  } else if (ratioCi[1] <= threshold || deltaInterval.ci95[1] <= absoluteThreshold) {
    status = 'no-regression';
    reason = 'the upper 95% bound stays below at least one materiality floor';
  } else {
    status = 'inconclusive';
    reason = 'uncertainty overlaps both materiality floors';
  }

  return {
    status,
    reason,
    ...coverage,
    minimum_samples: minimumSamples,
    threshold,
    absolute_threshold: absoluteThreshold,
    direction: 'higher-is-worse',
    estimate_ratio: logRatioInterval.estimate === null ? null : Math.exp(logRatioInterval.estimate),
    ratio_ci95: ratioCi,
    // Keep the former field name for existing report readers.
    ci95: ratioCi,
    estimate_delta: deltaInterval.estimate,
    delta_ci95: deltaInterval.ci95,
    ratios,
    deltas,
    log_ratio_sd: ratios.length > 1 ? summarize(ratios.map(Math.log)).sd : null,
    ratio_interval_method: 'paired-log-ratio two-sided 95% Student-t interval',
    delta_interval_method: deltaInterval.method,
  };
}

/** Combine only the explicitly designated user-impact metrics into a verdict. */
export function combinePrimaryVerdict(comparisons) {
  if (comparisons.length === 0) return 'inconclusive';
  if (comparisons.some(result => result.status === 'regression')) return 'regression';
  if (comparisons.some(result => result.status !== 'no-regression')) return 'inconclusive';
  return 'no-regression';
}
