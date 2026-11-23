#include "dep.h"

namespace acme {
struct Mine {};
template <class T> struct Box {};
} namespace legacy {
template <class T> struct traits<acme::Box<T>> {};
template <> constexpr int width<acme::Mine> = 4;
template <> int id<acme::Mine>(acme::Mine) { return 1; }
} namespace acme { namespace lib { template <> struct hash<Mine> {}; } } namespace legacy {
}
