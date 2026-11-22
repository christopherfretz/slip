#include "side.h"
// §8 PtrBase `Temporary`: a pointer to a lifetime-extended temporary against one to a variable.
constexpr const int& r1 = 1;
constexpr const int& r2 = 2;
int g;
#ifdef B
constexpr const int* p = &g;
#else
constexpr const int* p = &r1;
#endif
