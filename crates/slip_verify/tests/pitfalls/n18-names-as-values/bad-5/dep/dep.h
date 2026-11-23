namespace legacy {
struct Dep {};
template <unsigned long N> int length(const char (&)[N]) { return N; }
}
