namespace n {
template <class T> constexpr T v = T(1);
}
int h() { return n::v<int> + int(n::v<long>); }
