// WP-115 item 2 (digest §9): a default member initializer's use at an aggregate initialization in a namespace-scope initializer is numbered among that variable's uses: another variable that starts initializing S does not renumber s2's
#include "srcloc.h"
struct S { std::source_location l = std::source_location::current(); };
constexpr int s1 = 0;
constexpr S s2[2] = {S{}, S{}};
constexpr std::source_location G = s2[1].l;
