#include "side.h"
// §4.3 TypeView `Enum`.
enum E { e }; enum F { f };
#ifdef B
int v;
#else
E v;
#endif
