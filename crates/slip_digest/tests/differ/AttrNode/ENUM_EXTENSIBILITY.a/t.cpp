#include "side.h"
// §10.1 AttrNode: `ENUM_EXTENSIBILITY.a`: the fact differs.
#ifdef B
enum __attribute__((enum_extensibility(closed))) E { A };
#else
enum __attribute__((enum_extensibility(open))) E { A };
#endif
