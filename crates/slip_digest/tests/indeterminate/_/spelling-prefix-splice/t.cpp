// std: c++26
#include "side.h"
// §4.4 "Spellings", rule 11: a splice prefix (`[: r :]::`) in a body, its operand dependent: indeterminate (`Nns/Splice`), never read as if it held nothing.
struct S { static constexpr int value = 1; };
template<class T> int g(T) { return [: ^^T :]::value; }
