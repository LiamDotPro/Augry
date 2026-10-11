import { BRAND_ICONS, STROKE_ICONS } from "../../process/iconData";

/**
 * An icon from the design's set (icons/paths.js): a stroke icon drawn in
 * currentColor, or a filled brand mark in its brand colour. Marks that are
 * white in the design take currentColor, so they show on the light theme.
 */
export function ProcessIcon({
  name,
  size = 16,
  stroke = 1.7,
  className,
}: {
  name: string;
  size?: number;
  stroke?: number;
  className?: string;
}) {
  const brand = BRAND_ICONS[name];
  const path = brand ? null : (STROKE_ICONS as Record<string, string>)[name];
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {brand ? (
        <path d={brand.d} fill={brand.color ?? "currentColor"} />
      ) : path ? (
        <path
          d={path}
          fill="none"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      ) : null}
    </svg>
  );
}
