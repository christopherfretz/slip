namespace n { int h(int); template <class T> int fd(T); }
namespace n { template <class T> struct S { friend int h(int); template <class U> friend int fd(U); }; }
template struct n::S<int>;
int u() { return n::h(1) + n::fd(1); }
