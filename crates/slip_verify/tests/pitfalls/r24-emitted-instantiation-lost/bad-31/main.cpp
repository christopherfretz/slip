extern "C" int puts(const char *);
template <class T> struct A { static int k; };
template <class T> int A<T>::k = [] { struct L { __attribute__((used)) static void c() {} }; return 0; }();
struct Base { static int s(); };
namespace n { struct S : Base { template <class U = int> static auto h() { return (void)&A<U>::k, 1; } }; }
template <class T, int = sizeof(T::s())> int call() { return 0; }
int main() { return call<n::S>(); }
