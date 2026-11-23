// WP-115 item 5, H1 (digest §9): the same as name-value-signature-local-enum through std::source_location: the __impl object and its pretty name hold the local enum E the signature of L::m mentions
#include "srcloc.h"
consteval std::source_location f() { enum F { X }; struct L { static consteval std::source_location m(F) { return std::source_location::current(); } }; return L::m(X); }
constexpr std::source_location G = f();
