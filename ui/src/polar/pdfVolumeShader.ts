/** Proxy boxes cover the pixels only; intersections below define the actual cells. */
export const PDF_VOLUME_VERTEX = `
attribute vec3 pdfLower;
attribute vec3 pdfUpper;
attribute vec3 pdfParameters;
attribute float pdfOpacity;
attribute float cell;
attribute float windCell;
uniform float drawCell;
varying vec3 vPosition;
varying vec3 vLower;
varying vec3 vUpper;
varying vec3 vParameters;
varying float vOpacity;
varying float vWindCell;
varying float vCopy;
void main() {
  vPosition = position;
  vLower = pdfLower;
  vUpper = pdfUpper;
  vParameters = pdfParameters;
  vOpacity = pdfOpacity;
  vWindCell = windCell;
  vCopy = cell;
  if (drawCell > -0.5 && cell > -0.5 && abs(cell - drawCell) > 0.5) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

export const PDF_VOLUME_FRAGMENT = `
varying vec3 vPosition;
varying vec3 vLower;
varying vec3 vUpper;
varying vec3 vParameters;
varying float vOpacity;
varying float vWindCell;
varying float vCopy;
uniform mat4 projectionMatrix;
uniform float tower;
uniform float angleScale;
uniform float maxDensity;
uniform float hoverWindCell;
uniform float hoverCopy;

// Clip a ray to value + slope*t >= 0; parallel rays need no division.
bool clip(float value, float slope, inout vec2 interval) {
  if (abs(slope) < 0.0000001) return value >= -0.000001;
  float at = -value / slope;
  if (slope > 0.0) interval.x = max(interval.x, at);
  else interval.y = min(interval.y, at);
  return interval.y > interval.x;
}

// Up to two intervals: the inner cylinder can cut a hole through the ray.
bool intersectVolume(vec3 origin, vec3 direction, out vec4 segments) {
  vec2 interval = vec2(0.0, 1000000.0);
  if (tower < 0.5) {
    vec3 lo = vec3(vLower.x * angleScale, vLower.yz);
    vec3 hi = vec3(vUpper.x * angleScale, vUpper.yz);
    for (int axis = 0; axis < 3; axis++) {
      if (!clip(origin[axis] - lo[axis], direction[axis], interval)
        || !clip(hi[axis] - origin[axis], -direction[axis], interval)) return false;
    }
    segments = vec4(interval, interval.yy);
    return true;
  }
  if (!clip(origin.z - vLower.y, direction.z, interval)
    || !clip(vUpper.y - origin.z, -direction.z, interval)) return false;
  vec2 lowerNormal = vec2(cos(vLower.x), -sin(vLower.x));
  vec2 upperNormal = vec2(-cos(vUpper.x), sin(vUpper.x));
  if (!clip(dot(lowerNormal, origin.xy), dot(lowerNormal, direction.xy), interval)
    || !clip(dot(upperNormal, origin.xy), dot(upperNormal, direction.xy), interval)) return false;
  float a = dot(direction.xy, direction.xy);
  float b = dot(origin.xy, direction.xy);
  float radiusSquared = dot(origin.xy, origin.xy);
  if (a < 0.000000001) {
    if (radiusSquared < vLower.z * vLower.z || radiusSquared > vUpper.z * vUpper.z) return false;
    segments = vec4(interval, interval.yy);
    return true;
  }
  float outer = b*b - a*(radiusSquared - vUpper.z*vUpper.z);
  if (outer <= 0.0) return false;
  float root = sqrt(outer);
  interval.x = max(interval.x, (-b - root) / a);
  interval.y = min(interval.y, (-b + root) / a);
  if (interval.y <= interval.x) return false;
  segments = vec4(interval, interval.yy);
  float inner = b*b - a*(radiusSquared - vLower.z*vLower.z);
  if (inner > 0.0 && vLower.z > 0.0) {
    float holeRoot = sqrt(inner);
    float holeStart = (-b - holeRoot) / a;
    float holeEnd = (-b + holeRoot) / a;
    segments = vec4(interval.x, clamp(holeStart, interval.x, interval.y),
      clamp(holeEnd, interval.x, interval.y), interval.y);
  }
  return segments.y - segments.x + segments.w - segments.z > 0.00001;
}

vec3 pdfRamp(float t) {
  float x = clamp(t, 0.0, 1.0) * 4.0;
  if (x < 1.0) return mix(vec3(0.267, 0.005, 0.329), vec3(0.229, 0.322, 0.546), x);
  if (x < 2.0) return mix(vec3(0.229, 0.322, 0.546), vec3(0.128, 0.567, 0.551), x - 1.0);
  if (x < 3.0) return mix(vec3(0.128, 0.567, 0.551), vec3(0.369, 0.789, 0.383), x - 2.0);
  return mix(vec3(0.369, 0.789, 0.383), vec3(0.993, 0.906, 0.144), x - 3.0);
}

void main() {
  vec3 direction = normalize(vPosition - cameraPosition);
  vec4 segments;
  if (!intersectVolume(cameraPosition, direction, segments)) discard;
  float firstLength = segments.y - segments.x;
  float distance = firstLength + segments.w - segments.z;
  if (distance <= 0.00001) discard;
  float entry = firstLength > 0.00001 ? segments.x : segments.z;
  vec3 entryPoint = cameraPosition + direction * entry;
  vec4 projected = projectionMatrix * viewMatrix * vec4(entryPoint, 1.0);
  gl_FragDepth = clamp(0.5 * (projected.z / projected.w) + 0.5, 0.0, 1.0);

  // Integrate a continuous field along this pixel's ray. No internal planes
  // are drawn, and the final cell opacity does not depend on sample count.
  vec3 color = vec3(0.0);
  float accumulated = 0.0;
  float sigma = max(vParameters.y, 0.0001);
  for (int step = 0; step < 64; step++) {
    float travelled = (float(step) + 0.5) * distance / 64.0;
    float at = travelled < firstLength ? segments.x + travelled : segments.z + travelled - firstLength;
    vec3 point = cameraPosition + direction * at;
    float speed = tower > 0.5 ? length(point.xy) : point.z;
    float z = (speed - vParameters.x) / sigma;
    float density = exp(-0.5*z*z) / (sigma * 2.50662827 * max(vParameters.z, 0.0001));
    float normalized = clamp(density / max(maxDensity, 0.000001), 0.0, 1.0);
    float alpha = 1.0 - exp(-5.0 * sqrt(normalized) / 64.0);
    color += (1.0 - accumulated) * alpha * pdfRamp(normalized);
    accumulated += (1.0 - accumulated) * alpha;
  }
  color /= max(accumulated, 0.000001);
  bool hovered = abs(vWindCell - hoverWindCell) < 0.5 && (vCopy < -0.5 || abs(vCopy - hoverCopy) < 0.5);
  if (hovered) color = mix(color, vec3(1.0, 0.94, 0.52), 0.25);
  gl_FragColor = vec4(color, vOpacity);
}`;

export const PDF_OUTLINE_VERTEX = `
attribute float cell;
uniform float drawCell;
void main() {
  if (drawCell > -0.5 && cell > -0.5 && abs(cell - drawCell) > 0.5) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  // Keep an edge on its own boundary visible without drawing hidden edges
  // through unrelated cells in front of it.
  gl_Position.z -= 0.000001 * gl_Position.w;
}`;

export const PDF_OUTLINE_FRAGMENT = `
uniform vec3 color;
uniform float opacity;
void main() { gl_FragColor = vec4(color, opacity); }
`;
