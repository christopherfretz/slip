// std: c++17
#include "side.h"
// §10.2 DeclNameKind `Identifier`: an identifier against a literal operator of one suffix.
#ifdef B
void operator""_x(unsigned long long);
#else
void _x(unsigned long long);
#endif
