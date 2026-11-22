#include "side.h"
// §8 PtrBase `Temporary.0`: the temporary extended by `r1` against by `r2`.
constexpr const int& r1 = 1;
constexpr const int& r2 = 2;
#ifdef B
constexpr const int* p = &r2;
#else
constexpr const int* p = &r1;
#endif
