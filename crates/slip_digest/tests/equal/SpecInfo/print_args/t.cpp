#include "side.h"
// §9.4 SpecInfo `print_args`: the arguments a name prints with (trailing
// defaults dropped) are spelling.
template <class T, class U = int> struct X {};
#ifdef B
X<char, int> v;
#else
X<char> v;
#endif
