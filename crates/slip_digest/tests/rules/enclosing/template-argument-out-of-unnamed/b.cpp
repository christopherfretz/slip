template <class T> int g() { return sizeof(T); }
namespace { }
struct Impl { };
int u() { return g<Impl>(); }
