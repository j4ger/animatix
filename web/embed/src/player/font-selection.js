function covers(face, codepoint) {
  return (face.coverage ?? []).some(([start, end]) => codepoint >= start && codepoint <= end);
}

function key(value, fallback) {
  return String(value ?? fallback).toLowerCase();
}

function supportsStyle(face, style) {
  const normalize = (value) => ["italic", "oblique"].includes(value) ? "italic" : value;
  return normalize(key(face.style, "normal")) === normalize(style);
}

function selectForWeight(faces, weight) {
  const exactStatic = faces.filter((face) => Number(face.weight) === weight && !face.axes?.some(({ tag }) => tag === "wght"));
  if (exactStatic.length) return [exactStatic[0]];
  const variable = faces.filter((face) => {
    const axis = face.axes?.find(({ tag }) => tag === "wght");
    return axis && weight >= axis.min && weight <= axis.max;
  });
  if (variable.length) return [variable[0]];
  const distance = (face) => {
    const axis = face.axes?.find(({ tag }) => tag === "wght");
    if (axis) return weight < axis.min ? axis.min - weight : weight > axis.max ? weight - axis.max : 0;
    return Math.abs(Number(face.weight) - weight);
  };
  let nearest;
  for (const face of faces) {
    if (!nearest || distance(face) < distance(nearest)) nearest = face;
  }
  return nearest ? [nearest] : [];
}

function selectForStyle(faces, style) {
  const exact = faces.filter((face) => supportsStyle(face, style));
  if (exact.length) return exact;
  const normal = faces.filter((face) => supportsStyle(face, "normal"));
  if (normal.length) return normal;
  return faces.length ? [faces[0]] : [];
}

/** Resolve requested primary faces first, then add a coverage fallback per missing glyph. */
export function selectFontFaces(runtime, requirements) {
  const faces = runtime.manifest.fonts?.faces ?? [];
  const requestedFamilies = new Set((requirements?.families ?? []).map((name) => key(name, "")));
  const requestedWeights = [...new Set((requirements?.weights?.length ? requirements.weights : [400]).map(Number))];
  const requestedStyles = [...new Set((requirements?.styles?.length ? requirements.styles : ["normal"]).map((style) => key(style, "normal")))];
  const cps = [...new Set([...(requirements?.text ?? "")].map((char) => char.codePointAt(0)))];
  const dynamic = requirements?.dynamic === true;
  const families = requestedFamilies.size
    ? [...requestedFamilies]
    : (cps.length || dynamic ? ["open sans"] : []);
  if (requirements?.rich_text) {
    if (!families.includes("open sans")) families.push("open sans");
    if (!families.includes("fira math") && faces.some((face) => key(face.family, "") === "fira math")) families.push("fira math");
  }
  if (!families.length) return [];

  const selected = new Set();
  for (const family of families) {
    const available = faces.filter((face) => key(face.family, "") === family);
    if (dynamic || requirements?.rich_text) {
      for (const face of available) selected.add(face.id);
    } else {
      for (const requestedStyle of requestedStyles) {
        const styleFaces = selectForStyle(available, requestedStyle);
        for (const weight of requestedWeights) {
          for (const face of selectForWeight(styleFaces, weight)) selected.add(face.id);
        }
      }
    }
  }

  const fallbackIds = runtime.manifest.fonts?.fallbacks ?? [];
  const fallbackFaces = fallbackIds.map((id) => faces.find((face) => face.id === id)).filter(Boolean);
  for (const cp of cps) {
    if ([...selected].some((id) => {
      const face = faces.find((candidate) => candidate.id === id);
      return face && covers(face, cp);
    })) continue;
    const fallback = fallbackFaces.find((face) => covers(face, cp));
    if (fallback) selected.add(fallback.id);
  }
  if (dynamic) for (const face of fallbackFaces) selected.add(face.id);
  return faces.filter((face) => selected.has(face.id));
}
