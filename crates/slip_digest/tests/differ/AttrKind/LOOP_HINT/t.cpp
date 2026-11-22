#include "side.h"
// §10.1 AttrKind: `LOOP_HINT`: the attribute on one side only.
#ifdef B
void f() {
#pragma clang loop unroll(enable)
  for (;;) { }
}
#else
void f() { for (;;) { } }
#endif
