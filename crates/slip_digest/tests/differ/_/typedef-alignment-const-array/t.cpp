#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an array of a cv-qualified typedef of the typedef; the plain typedef `CL` holds it too.
typedef long L __attribute__((aligned(4)));
#ifdef B
typedef const L CL;
struct S { char c; CL a[2]; };
#else
typedef const long CL;
struct S { char c; CL a[2]; };
#endif
