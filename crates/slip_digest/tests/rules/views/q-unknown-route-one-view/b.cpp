namespace other { template <class T> int g(T); }
namespace acme { using other::g; }
template <class T> int f(T t) { return acme::g(t); }
