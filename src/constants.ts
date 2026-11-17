// Keyless Google Maps raster tiles, as used by the browser reference app.
export const GOOGLE_ROADMAP =
  'https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m2!1e0!2sm!3m7!2sen!3sus!5e1105!12m1!1e3!12m1!1e2!4e0!5m5!1e0!8m2!1e1!1e1!8i47083502!6m6!1e12!2i2!11e0!39b0!44e0!50e0'
export const GOOGLE_TERRAIN =
  'https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m2!1e0!2sm!2m1!1e4!3m7!2sen!3scn!5e1105!12m1!1e67!12m1!1e2!4e0!5m5!1e0!8m2!1e1!1e1!8i47083502!6m6!1e12!2i2!11e0!39b0!44e0!50e0'
export const GOOGLE_SATELLITE =
  'https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m2!1e1!2sm!3m5!2sen!3scn!5e1105!12m1!1e2!4e0!5m5!1e0!8m2!1e1!1e1!8i47083502!6m6!1e12!2i2!11e0!39b0!44e0!50e0'
export const GOOGLE_LABELS =
  'https://maps.googleapis.com/maps/vt?pb=%211m5%211m4%211i{z}%212i{x}%213i{y}%214i256%212m2%211e0%212sm%213m17%212sen%213scn%215e1105%2112m4%211e68%212m2%211sset%212sRoadmap%2112m3%211e37%212m1%211ssmartmaps%2112m4%211e26%212m2%211sstyles%212ss.t%3A18%7Cs.e%3Ag.s%7Cp.w%3A3%2Cs.e%3Ag%7Cp.v%3Aoff%2Cs.t%3A1%7Cs.e%3Ag.s%7Cp.v%3Aon%2Cs.e%3Al%7Cp.v%3Aon%214i0%215m2%211e0%215f1.5'
export const GOOGLE_LABELS_SATELLITE =
  'https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m2!1e0!2sm!3m5!2sen!3scn!5e1105!12m1!1e4!4e0!5m5!1e0!8m2!1e1!1e1!8i47083502!6m6!1e12!2i2!11e0!39b0!44e0!50e0'

const STREETVIEW_STYLES =
  '%212m2%211sstyles%212sp.c%3A%23{strokeColor}%2Cs.e%3Ag.f%7Cp.c%3A%23{strokeColor}%7Cp.w%3A{lineWidth}%2Cs.e%3Ag.s%7Cp.c%3A%23{fillColor}%7Cp.w%3A{strokeWidth}%215m1%215f1.35'
const STREETVIEW_OFFICIAL =
  'https://maps.googleapis.com/maps/vt?pb=%211m5%211m4%211i{z}%212i{x}%213i{y}%214i256%212m8%211e2%212ssvv%214m2%211scc%212s*211m3*211e2*212b1*213e2*212b1%214m2%211ssvl%212s*212b1%213m16%212sen%213sUS%2112m4%211e68%212m2%211sset%212sRoadmap%2112m3%211e37%212m1%211ssmartmaps%2112m4%211e26'
const STREETVIEW_ALL =
  'https://maps.googleapis.com/maps/vt?pb=%211m5%211m4%211i{z}%212i{x}%213i{y}%214i256%212m8%211e2%212ssvv%214m2%211scc%212s*211m3*211e2*212b1*213e2*211m3*211e3*212b1*213e2*211m3*211e10*212b1*213e2*212b1%214m2%211ssvl%212s*212b1%213m16%212sen%213sUS%2112m4%211e68%212m2%211sset%212sRoadmap%2112m3%211e37%212m1%211ssmartmaps%2112m4%211e26'
const STREETVIEW_BLOBBY =
  'https://maps.googleapis.com/maps/vt?pb=!1m5!1m4!1i{z}!2i{x}!3i{y}!4i256!2m8!1e2!2ssvv%214m2%211scc%212s*211m3*211e2*212b1*213e2*212b1%214m2%211ssvl%212s*21%213m16%212sen%213sUS%2112m4%211e68%212m2%211sset%212sRoadmap%2112m3%211e37%212m1%211ssmartmaps%2112m4%211e26%212m2%211sstyles%212sp.c%3A%23{strokeColor}%215m1%215f1.35'

export const COLOR_SCHEMES: Record<string, [string, string]> = {
  Default: ['1098ad', '99e9f2'],
  Crimson: ['f03e3e', 'ffc9c9'],
  Deep_Pink: ['d6336c', 'fcc2d7'],
  Blue_Violet: ['ae3ec9', 'eebefa'],
  Slate_Blue: ['7048e8', 'd0bfff'],
  Royal_Blue: ['4263eb', 'bac8ff'],
  Dodger_Blue: ['1c7ed6', 'a5d8ff'],
  Sea_Green: ['0ca678', '96f2d7'],
  Lime_Green: ['37b24d', 'b2f2bb'],
  Olive_Drab: ['74b816', 'd8f5a2'],
  Orange: ['f59f00', 'ffec99'],
  Dark_Orange: ['f76707', 'ffd8a8'],
  Brown: ['bd5f1b', 'f7ca9e'],
}

export interface CoverageStyle {
  official: boolean
  blobby?: boolean
  scheme?: string
  lineWidth?: number
  strokeWidth?: number
}

export function streetViewUrl(style: CoverageStyle): string {
  const [stroke, fill] = COLOR_SCHEMES[style.scheme || 'Default'] || COLOR_SCHEMES.Default
  if (style.blobby) {
    return STREETVIEW_BLOBBY.replaceAll('{strokeColor}', stroke)
  }
  const base = style.official ? STREETVIEW_OFFICIAL : STREETVIEW_ALL
  const lineWidth = style.lineWidth ?? 1
  const strokeWidth = style.strokeWidth ?? 3
  return (base + STREETVIEW_STYLES)
    .replaceAll('{strokeColor}', stroke)
    .replaceAll('{fillColor}', fill)
    .replaceAll('{lineWidth}', String(lineWidth))
    .replaceAll('{strokeWidth}', String(strokeWidth))
}
