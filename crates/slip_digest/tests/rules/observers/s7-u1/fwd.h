#ifdef POST
namespace fresh { template <class T> struct box; struct Mine {};
template <> struct box<Mine> { int v; }; }
#else
namespace old { template <class T> struct box; struct Mine {};
template <> struct box<Mine> { int v; }; }
#endif
