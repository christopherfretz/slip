#include "side.h"
// §5.3 DeclKind `ConstructorUsingShadow`: through the inheriting using-declaration: a second constructor shadow.
#ifdef B
struct P { P(int); P(long); };
#else
struct P { P(int); };
#endif
struct D : P { using P::P; };
