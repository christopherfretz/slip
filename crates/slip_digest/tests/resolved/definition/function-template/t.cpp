#include "side.h"
// digest-kinds §11.9: a function template specialisation named in an
// unevaluated operand is declared; the call defines it, in `definition`.
template <class T> T f(T x) { return x; }
decltype(f(1)) y;
#ifdef B
int h() { return f(1); }
#endif
