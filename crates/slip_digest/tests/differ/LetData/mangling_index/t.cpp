#include "side.h"
// §5.4 LetData `mangling_index`.
#ifdef B
struct P { const int& a; const int& b; };
P r{1, 2};
#else
struct P { const int& a; const int& b; };
P r{1, 1};
#endif
