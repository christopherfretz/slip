#include "side.h"
// §5.4 VarSpec `0-constexpr`.
#ifdef B
constexpr int x = 1;
#else
const int x = 1;
#endif
