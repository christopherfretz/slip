#include "side.h"
// §9.4 SpecInfo `print_args`: the arguments a name prints with (trailing
// defaults dropped) are spelling.
template <class T, class U = int> struct X {};
#ifdef B
template struct X<char, int>;
#else
template struct X<char>;
#endif
