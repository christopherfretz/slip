#define SPECIALISE_F template <> void f<int>(int) {}
namespace n {
template <class T> void f(T) {}
SPECIALISE_F
}
