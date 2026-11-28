namespace std {
template <class T> T&& forward(T& t) noexcept;
template <class T> T&& forward(T&& t) noexcept;
}
