#include "side.h"
// WP-034 (§6): comments and layout.
#ifdef B
void f(int x)
{
  // b
  if (x)
    return;
}
#else
void f(int x) { if (x) /* a */ return; }
#endif
