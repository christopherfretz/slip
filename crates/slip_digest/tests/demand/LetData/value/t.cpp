#include "side.h"
// §5.4 LetData `value`, §11.3.
#ifdef B
constexpr const int& r = 1;
constexpr int y = r;
#else
constexpr const int& r = 1;
#endif
