#define NAME f
namespace n {
template <class T> void f(T) {}
template <> void NAME<int>(int) {}
}
void h() { n::f(1); }
