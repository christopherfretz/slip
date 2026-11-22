#include "side.h"
// §10.1 AttrKind: `FLAG_ENUM`: the attribute on one side only.
#ifdef B
enum __attribute__((flag_enum)) E { A = 1 };
#else
enum E { A = 1 };
#endif
