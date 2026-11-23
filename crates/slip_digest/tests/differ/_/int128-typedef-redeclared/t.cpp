#include "side.h"
// WP-050: Kiln declares `__int128_t` without a location in every
// translation unit of a 64-bit target; a program's typedef redeclares it
// and is a declared unit of this file (digest §11 origin file). Unwritten,
// it is no unit.
#ifdef B
typedef __int128 __int128_t;
#endif
int x;
