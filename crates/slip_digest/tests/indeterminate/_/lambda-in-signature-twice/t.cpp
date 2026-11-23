#include "side.h"
// §4.4 "Kinds walked in full" (F-E): a lambda in a signature makes each declaration its own template in Kiln ([temp.over.link]/5: never equivalent), and the digest writes one key for both, whose bytes are the lambda's spelling in full. The key's two entities are a `collision`: indeterminate, never a silent merge.
template<int N> void foo(const char(*)[([]{}, N)]);
template<int N> void foo(const char(*)[([]{}, N)]);
