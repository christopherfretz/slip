#include "side.h"
// §5.4 LetData `value`, §11.3.
#ifdef B
constexpr const int& r = 1;
int y() { constexpr int k = r; return k; }
#else
constexpr const int& r = 1;
#endif
