extern "C" int puts(const char *);
inline int note() { puts("registered"); return 1; }
template <class T> struct Reg { static int r; };
template <class T> int Reg<T>::r = note();
struct Base { static int s(); };
namespace n {
struct S : Base {
  template <class U = int> static auto h() { return (void)&Reg<U>::r, 1; }
};
}
template <class T, int = sizeof(T::s(), T::nope())> int call(int) { return 1; }
template <class T> int call(long) { return 0; }
int main() { return call<n::S>(0); }
