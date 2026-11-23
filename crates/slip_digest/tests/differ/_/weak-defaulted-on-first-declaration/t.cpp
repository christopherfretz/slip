#include "side.h"
// digest-kinds §11.9 (WP-045 review F1): a function defaulted on its first
// declaration is of the definition class (rule 3), and that declaration is
// written in source: its `weak` stays in the own unit.
struct S {
#ifdef B
  __attribute__((weak))
#endif
  S() = default;
  int x;
};
