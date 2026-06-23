use super::*;

impl App {
    pub(super) fn handle_number_jump_digit(&mut self, c: char) {
        self.number_jump.push_digit(c);
    }

    pub(super) fn handle_number_jump_confirm(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.number_jump.clear();
            return;
        }

        let target = self.number_jump.target_row();
        self.ui.nav.selected = if target == 0 {
            0
        } else if target > count {
            count - 1
        } else {
            target - 1
        };

        self.number_jump.clear();
    }

    pub(super) fn handle_number_jump_cancel(&mut self) {
        self.number_jump.clear();
    }

    pub(super) fn check_number_jump_timeout(&mut self, now: std::time::Instant) {
        if self.number_jump.is_active() && self.number_jump.is_expired(now) {
            self.handle_number_jump_cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn app_with_stations(count: usize) -> App {
        let stations: Vec<Station> = (0..count)
            .map(|i| station(&format!("Station {i}"), &format!("http://{i}")))
            .collect();
        App::new(Library::in_memory(stations))
    }

    #[test]
    fn digit_accumulation_builds_target() {
        let mut app = app_with_stations(20);

        app.handle_number_jump_digit('1');
        app.handle_number_jump_digit('5');

        assert!(app.number_jump.is_active());
        assert_eq!(app.number_jump.display(), "15");
        assert_eq!(app.number_jump.target_row(), 15);
    }

    #[test]
    fn confirm_jumps_to_correct_row() {
        let mut app = app_with_stations(20);
        app.ui.nav.selected = 0;

        app.handle_number_jump_digit('7');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 6); // 1-based 7 → 0-based 6
        assert!(!app.number_jump.is_active());
    }

    #[test]
    fn confirm_with_zero_goes_to_first() {
        let mut app = app_with_stations(10);
        app.ui.nav.selected = 5;

        app.handle_number_jump_digit('0');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn confirm_exceeding_count_clamps_to_last() {
        let mut app = app_with_stations(5);
        app.ui.nav.selected = 0;

        app.handle_number_jump_digit('9');
        app.handle_number_jump_digit('9');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 4); // count - 1
    }

    #[test]
    fn empty_list_discards_jump() {
        let mut app = app_with_stations(0);

        app.handle_number_jump_digit('3');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 0);
        assert!(!app.number_jump.is_active());
    }

    #[test]
    fn cancel_clears_state() {
        let mut app = app_with_stations(10);

        app.handle_number_jump_digit('4');
        app.handle_number_jump_digit('2');
        assert!(app.number_jump.is_active());

        app.handle_number_jump_cancel();

        assert!(!app.number_jump.is_active());
        assert_eq!(app.number_jump.display(), "");
    }

    #[test]
    fn timeout_cancels_accumulation() {
        let mut app = app_with_stations(10);

        app.handle_number_jump_digit('5');
        assert!(app.number_jump.is_active());

        let future = std::time::Instant::now()
            + std::time::Duration::from_millis(crate::number_jump::NUMBER_JUMP_TIMEOUT_MS + 100);
        app.check_number_jump_timeout(future);

        assert!(!app.number_jump.is_active());
    }

    #[test]
    fn confirm_with_target_one_selects_first_station() {
        let mut app = app_with_stations(10);
        app.ui.nav.selected = 5;

        app.handle_number_jump_digit('1');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn confirm_with_target_equal_to_count_selects_last() {
        let mut app = app_with_stations(5);
        app.ui.nav.selected = 0;

        app.handle_number_jump_digit('5');
        app.handle_number_jump_confirm();

        assert_eq!(app.ui.nav.selected, 4);
    }
}
