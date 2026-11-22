#include "side.h"
// §5.5 FieldData `bit_width`: a bit-field against a plain field.
#ifdef B
struct C { int a : 3; };
#else
struct C { int a; };
#endif
