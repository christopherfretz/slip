namespace legacy { int h(int); int h(double); }
namespace acme { template <class T> auto f(T t) -> decltype(legacy::h(t)); }
