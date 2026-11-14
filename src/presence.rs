#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Presence {
    pub playing: bool,
    pub controls_wanted: bool,
    pub dwelt: bool,
    pub hovered: bool,
    pub hidden: bool,
    pub opacity: f32,
    pub hover_opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub visible: bool,
    pub opacity: f32,
    pub takes_clicks: bool,
    pub controls: bool,
}

impl Presence {
    fn interactive(&self) -> bool {
        !self.playing || self.controls_wanted || self.dwelt
    }

    fn see_through(&self) -> bool {
        self.hovered && !self.interactive()
    }

    pub fn look(&self) -> Look {
        Look {
            visible: !self.hidden,
            opacity: if self.see_through() { self.hover_opacity } else { self.opacity_when_shown() },
            takes_clicks: !self.hidden && self.interactive(),
            controls: self.interactive(),
        }
    }

    fn opacity_when_shown(&self) -> f32 {
        if self.interactive() { 1.0 } else { self.opacity }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATCHING: Presence = Presence {
        playing: true,
        controls_wanted: false,
        dwelt: false,
        hovered: false,
        hidden: false,
        opacity: 1.0,
        hover_opacity: 0.15,
    };

    #[test]
    fn while_watching_it_never_takes_clicks() {
        assert!(!WATCHING.look().takes_clicks);
        assert!(!Presence { hovered: true, ..WATCHING }.look().takes_clicks);
    }

    #[test]
    fn hovering_while_watching_makes_it_see_through() {
        assert_eq!(Presence { hovered: true, ..WATCHING }.look().opacity, 0.15);
        assert_eq!(WATCHING.look().opacity, 1.0);
    }

    #[test]
    fn asking_or_resting_the_pointer_brings_controls_back_fully_visible() {
        let asked = Presence { hovered: true, controls_wanted: true, ..WATCHING }.look();
        let held = Presence { hovered: true, dwelt: true, ..WATCHING }.look();
        assert_eq!(asked, Look { visible: true, opacity: 1.0, takes_clicks: true, controls: true });
        assert_eq!(held, asked);
    }

    #[test]
    fn with_nothing_playing_it_can_be_clicked_to_pick_something() {
        assert!(Presence { playing: false, ..WATCHING }.look().takes_clicks);
    }

    #[test]
    fn hidden_takes_no_clicks_even_with_controls_wanted() {
        let look = Presence { hidden: true, controls_wanted: true, ..WATCHING }.look();
        assert!(!look.visible);
        assert!(!look.takes_clicks);
    }

    #[test]
    fn a_dimmed_window_stays_dimmed_until_hovered() {
        let dimmed = Presence { opacity: 0.6, ..WATCHING };
        assert_eq!(dimmed.look().opacity, 0.6);
        assert_eq!(Presence { hovered: true, ..dimmed }.look().opacity, 0.15);
    }
}
