#include "side.h"
// §7.3 CompoundAssignExtra `lhs_ty`: `b += p` for a `bool b` computes the
// promoted `b` plus `p`, so the left computation type is `int` and the
// result one `int *` (kiln_sema expr.rs:4465, clang's `CompLHSTy`).
bool b;
int *p;
#ifdef B
bool v = (b += 1);
#else
bool v = (b += p);
#endif
