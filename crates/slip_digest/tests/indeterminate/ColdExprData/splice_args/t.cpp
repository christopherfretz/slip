// std: c++26
#include "side.h"
// §7.4 ColdExprData `splice_args`.
// a dependent splice template-id
template<auto R> int v = template [:R:]<int>;
