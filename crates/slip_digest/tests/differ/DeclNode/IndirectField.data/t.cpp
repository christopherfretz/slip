#include "side.h"
// §5.3 DeclNode `IndirectField.data`: the chain of fields.
#ifdef B
struct S { union { union { int a; }; }; };
#else
struct S { union { int a; }; };
#endif
