export type SelectValue = string | number | boolean

export interface SelectOption<T extends SelectValue = SelectValue> {
  value: T
  label: string
  disabled?: boolean
  colors?: [string, string]
}
