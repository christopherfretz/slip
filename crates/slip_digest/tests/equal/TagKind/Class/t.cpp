#include "side.h"
// §5.5 TagKind `Class`: written as Struct (digest §1 rule 2); the effective access of a base and of a member are the same.
#ifdef B
struct P {}; class C : public P { public: int a; };
#else
struct P {}; struct C : P { int a; };
#endif
