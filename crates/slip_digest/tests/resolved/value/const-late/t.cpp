#include "side.h"
// digest-kinds §11.3: a const integral variable with a constant
// initialiser, evaluated at its declaration.
const int n = 3;
#ifdef B
int h() { int a[n]; return sizeof a; }
#endif
