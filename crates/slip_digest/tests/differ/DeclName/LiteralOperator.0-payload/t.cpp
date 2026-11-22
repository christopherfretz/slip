// std: c++17
#include "side.h"
// §10.2 DeclName `LiteralOperator.0-payload`: the suffix.
#ifdef B
void operator""_b(unsigned long long);
#else
void operator""_a(unsigned long long);
#endif
