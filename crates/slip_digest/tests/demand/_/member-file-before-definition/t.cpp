#include "side.h"
// digest-kinds §11.1 (WP-045 fix round, item 4): Kiln moves a member it
// instantiates the definition of to the pattern's definition
// (kiln_ast/src/templates.rs:1989); the declared unit X<int>::g() is in
// the file of the declaration it was instantiated from, cls.h, on both
// sides.
#include "cls.h"
extern template struct X<int>;
#ifdef B
int use(X<int>& v) { return v.g(); }
#endif
#include "def.h"
