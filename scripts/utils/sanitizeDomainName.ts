/**
 * Sanitizes a given domain name by stripping whitespace and lowercasing it.
 */
export interface SanitizeDomainNameOptions {
  trim?: boolean
  lowercase?: boolean
  replaceUnderscores?: boolean
  removeOuterNonAlphanumeric?: boolean
  removeDots?: boolean
}
export const sanitizeDomainName = (value?: string, options?: SanitizeDomainNameOptions): string => {
  // Merge default options
  const _o = Object.assign(
    {
      trim: true,
      lowercase: true,
    } satisfies SanitizeDomainNameOptions,
    options,
  )

  // Sanitize value
  let _value = value || ''
  if (_o.trim) _value = _value.trim()
  if (_o.lowercase) _value = _value.toLowerCase()
  if (_o.replaceUnderscores) _value = _value.replaceAll('_', '-')
  if (_o.removeOuterNonAlphanumeric)
    _value = _value = _value.replace(/^[^a-z0-9]+|[^a-z0-9]+$/g, '')

  return _value
}
