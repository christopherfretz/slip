#include "side.h"
// digest-kinds §11.3: a reference that is not constexpr and a temporary
// whose value Kiln never writes: no resolved unit on either side.
const int &r = 1 + 2;
#ifdef B
int h() { return r; }
#endif
