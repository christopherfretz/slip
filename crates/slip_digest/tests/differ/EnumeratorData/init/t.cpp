#include "side.h"
// §5.5 EnumeratorData `init`: an initialiser of the same value against none.
#ifdef B
enum E { a = 0 };
#else
enum E { a };
#endif
