#include "side.h"
// §5.3 DeclNode `Field.data`: record FieldData.
#ifdef B
struct C { int a : 3; };
#else
struct C { int a; };
#endif
