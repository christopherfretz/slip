// std: c++17
#include "side.h"
// §10.2 DeclNameKind `LiteralOperator`: a literal operator against an identifier of one spelling.
#ifdef B
void _x(unsigned long long);
#else
void operator""_x(unsigned long long);
#endif
