#include "side.h"
// §5.3 DeclKind `BuiltinTemplate`: added: a use of the builtin template.
template <class T, T...> struct Seq {};
#ifdef B
using X = __make_integer_seq<Seq, int, 2>;
#endif
