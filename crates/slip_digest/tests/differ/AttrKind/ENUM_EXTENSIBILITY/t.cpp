#include "side.h"
// §10.1 AttrKind: `ENUM_EXTENSIBILITY`: the attribute on one side only.
#ifdef B
enum __attribute__((enum_extensibility(open))) E { A };
#else
enum E { A };
#endif
