#include "side.h"
// §5.5 EnumData `members`: two enumerators swapped (digest §5: enumerator order is kept).
#ifdef B
enum E { b, a };
#else
enum E { a, b };
#endif
