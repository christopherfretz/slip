#include "side.h"
// digest-kinds §11.9 (WP-045 review F1): Kiln gives a function definition
// the `#pragma clang section text` in force at its declarator
// (`kiln_sema/src/section.rs:188`), `= default` included; the declaration
// is written in source, so the attribute stays in the own unit.
#ifdef B
#pragma clang section text=".x"
#endif
struct S {
  S() = default;
  int x;
};
#pragma clang section text=""
