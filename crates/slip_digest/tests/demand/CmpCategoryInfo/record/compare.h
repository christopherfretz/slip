namespace std {
struct partial_ordering { int v; static const partial_ordering less, equivalent, greater, unordered; };
inline constexpr partial_ordering partial_ordering::less{-1}, partial_ordering::equivalent{0},
  partial_ordering::greater{1}, partial_ordering::unordered{2};
struct weak_ordering { int v; static const weak_ordering less, equivalent, greater;
  friend constexpr bool operator==(weak_ordering a, int) { return a.v == 0; } };
inline constexpr weak_ordering weak_ordering::less{-1}, weak_ordering::equivalent{0},
  weak_ordering::greater{1};
struct strong_ordering { int v; static const strong_ordering less, equal, equivalent, greater;
  constexpr operator weak_ordering() const { return weak_ordering{v}; } };
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::equivalent{0}, strong_ordering::greater{1};
}
