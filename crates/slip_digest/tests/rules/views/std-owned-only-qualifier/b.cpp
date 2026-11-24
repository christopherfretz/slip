namespace std { int h(int); int h(long); }
template <class T> int f(T t) { return std::h(t); }
int* g() { return new int; }
