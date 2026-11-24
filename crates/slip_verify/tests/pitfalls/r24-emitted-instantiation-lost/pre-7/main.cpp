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
template <class...> using void_t = void;
template <class T, class = void> struct has { static constexpr int v = 0; };
template <class T>
struct has<T, void_t<decltype(T::s()), typename T::nope>> { static constexpr int v = 1; };
int main() { return has<n::S>::v; }
