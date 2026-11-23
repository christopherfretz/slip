#include "side.h"
// digest-kinds §11.3: the value of the temporary `r` extends is in r's
// resolved unit, after r's own.
constexpr const int &r = 1 + 2;
#ifdef B
int h() { return r; }
#endif
