//! User profile module.
//!
//! Maintains a comprehensive user profile built from interactions.

use crate::{EventType, LearningEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Comprehensive user profile built from learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Unique profile ID.
    pub id: Uuid,
    /// Profile creation date.
    pub created_at: DateTime<Utc>,
    /// Last updated date.
    pub updated_at: DateTime<Utc>,
    /// Experience level (0.0 = beginner, 1.0 = expert).
    pub experience_level: f64,
    /// Primary programming languages used.
    pub primary_languages: Vec<String>,
    /// Language usage counts.
    pub language_usage: HashMap<String, usize>,
    /// Framework usage counts.
    pub framework_usage: HashMap<String, usize>,
    /// Total messages sent.
    pub total_messages: usize,
    /// Total code generations.
    pub total_code_generations: usize,
    /// Total tasks completed.
    pub total_tasks_completed: usize,
    /// Total time spent (estimated, in minutes).
    pub total_time_minutes: u64,
    /// Expertise areas.
    pub expertise_areas: Vec<ExpertiseArea>,
    /// Interests detected from conversations.
    pub interests: Vec<String>,
    /// Common project types.
    pub project_types: HashMap<String, usize>,
    /// Skill assessments.
    pub skills: HashMap<String, SkillLevel>,
    /// Achievements earned.
    pub achievements: Vec<Achievement>,
    /// Streak information.
    pub streaks: StreakInfo,
}

impl UserProfile {
    /// Create a new user profile.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            experience_level: 0.5,
            primary_languages: Vec::new(),
            language_usage: HashMap::new(),
            framework_usage: HashMap::new(),
            total_messages: 0,
            total_code_generations: 0,
            total_tasks_completed: 0,
            total_time_minutes: 0,
            expertise_areas: Vec::new(),
            interests: Vec::new(),
            project_types: HashMap::new(),
            skills: HashMap::new(),
            achievements: Vec::new(),
            streaks: StreakInfo::default(),
        }
    }

    /// Update profile from a learning event.
    pub fn update_from_event(&mut self, event: &LearningEvent) {
        self.updated_at = Utc::now();

        match event.event_type {
            EventType::MessageSent => {
                self.total_messages += 1;
                self.update_experience_from_messages();
            }
            EventType::CodeGenerated => {
                self.total_code_generations += 1;
            }
            EventType::TaskCompleted => {
                self.total_tasks_completed += 1;
                self.check_achievements();
            }
            _ => {}
        }

        // Update language usage.
        if let Some(lang) = &event.context.language {
            *self.language_usage.entry(lang.clone()).or_insert(0) += 1;
            self.update_primary_languages();
        }

        // Update project types.
        if let Some(project_type) = &event.context.project_type {
            *self.project_types.entry(project_type.clone()).or_insert(0) += 1;
        }

        // Update streaks.
        self.streaks.record_activity(event.timestamp);
    }

    /// Get profile completeness (0.0 - 1.0).
    pub fn completeness(&self) -> f64 {
        let mut score = 0.0;
        let mut max_score = 0.0;

        // Messages (up to 100).
        max_score += 1.0;
        score += (self.total_messages as f64 / 100.0).min(1.0);

        // Languages (at least 1).
        max_score += 1.0;
        if !self.primary_languages.is_empty() {
            score += 1.0;
        }

        // Tasks completed (at least 10).
        max_score += 1.0;
        score += (self.total_tasks_completed as f64 / 10.0).min(1.0);

        // Skills (at least 3).
        max_score += 1.0;
        score += (self.skills.len() as f64 / 3.0).min(1.0);

        // Expertise areas (at least 1).
        max_score += 1.0;
        if !self.expertise_areas.is_empty() {
            score += 1.0;
        }

        score / max_score
    }

    /// Get experience title based on level.
    pub fn experience_title(&self) -> &'static str {
        match self.experience_level {
            x if x < 0.2 => "Beginner",
            x if x < 0.4 => "Learner",
            x if x < 0.6 => "Intermediate",
            x if x < 0.8 => "Advanced",
            _ => "Expert",
        }
    }

    /// Get skill level for a topic.
    pub fn skill_level(&self, topic: &str) -> Option<&SkillLevel> {
        self.skills.get(topic)
    }

    /// Add or update a skill.
    pub fn update_skill(&mut self, topic: &str, level: f64) {
        let skill = self.skills.entry(topic.to_string()).or_insert_with(|| SkillLevel {
            topic: topic.to_string(),
            level: 0.0,
            last_used: Utc::now(),
            usage_count: 0,
        });
        
        // Exponential moving average for smoother updates.
        skill.level = skill.level * 0.8 + level * 0.2;
        skill.last_used = Utc::now();
        skill.usage_count += 1;
    }

    /// Get top N skills.
    pub fn top_skills(&self, n: usize) -> Vec<(&String, &SkillLevel)> {
        let mut skills: Vec<_> = self.skills.iter().collect();
        skills.sort_by(|a, b| b.1.level.partial_cmp(&a.1.level).unwrap());
        skills.truncate(n);
        skills
    }

    fn update_primary_languages(&mut self) {
        let mut languages: Vec<_> = self.language_usage.iter().collect();
        languages.sort_by(|a, b| b.1.cmp(a.1));
        
        self.primary_languages = languages
            .into_iter()
            .take(3)
            .map(|(k, _)| k.clone())
            .collect();
    }

    fn update_experience_from_messages(&mut self) {
        // Gradually increase experience based on usage.
        let progress = (self.total_messages as f64 / 1000.0).min(0.3);
        let tasks_progress = (self.total_tasks_completed as f64 / 100.0).min(0.3);
        let code_progress = (self.total_code_generations as f64 / 500.0).min(0.2);
        let skill_progress = (self.skills.len() as f64 / 20.0).min(0.2);
        
        self.experience_level = (progress + tasks_progress + code_progress + skill_progress).min(1.0);
    }

    fn check_achievements(&mut self) {
        // First task.
        if self.total_tasks_completed == 1 && !self.has_achievement("first_task") {
            self.achievements.push(Achievement {
                id: "first_task".to_string(),
                name: "First Steps".to_string(),
                description: "Completed your first task".to_string(),
                earned_at: Utc::now(),
                tier: AchievementTier::Bronze,
            });
        }

        // 10 tasks.
        if self.total_tasks_completed == 10 && !self.has_achievement("ten_tasks") {
            self.achievements.push(Achievement {
                id: "ten_tasks".to_string(),
                name: "Getting Started".to_string(),
                description: "Completed 10 tasks".to_string(),
                earned_at: Utc::now(),
                tier: AchievementTier::Bronze,
            });
        }

        // 100 tasks.
        if self.total_tasks_completed == 100 && !self.has_achievement("hundred_tasks") {
            self.achievements.push(Achievement {
                id: "hundred_tasks".to_string(),
                name: "Power User".to_string(),
                description: "Completed 100 tasks".to_string(),
                earned_at: Utc::now(),
                tier: AchievementTier::Silver,
            });
        }

        // Polyglot (3+ languages).
        if self.primary_languages.len() >= 3 && !self.has_achievement("polyglot") {
            self.achievements.push(Achievement {
                id: "polyglot".to_string(),
                name: "Polyglot".to_string(),
                description: "Used 3 or more programming languages".to_string(),
                earned_at: Utc::now(),
                tier: AchievementTier::Silver,
            });
        }

        // Week streak.
        if self.streaks.current_streak >= 7 && !self.has_achievement("week_streak") {
            self.achievements.push(Achievement {
                id: "week_streak".to_string(),
                name: "Consistent".to_string(),
                description: "Used Ember for 7 consecutive days".to_string(),
                earned_at: Utc::now(),
                tier: AchievementTier::Gold,
            });
        }
    }

    fn has_achievement(&self, id: &str) -> bool {
        self.achievements.iter().any(|a| a.id == id)
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// An area of expertise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertiseArea {
    /// Area name.
    pub name: String,
    /// Confidence level (0.0 - 1.0).
    pub confidence: f64,
    /// Evidence for this expertise.
    pub evidence: Vec<String>,
}

/// Skill level for a specific topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLevel {
    /// Topic name.
    pub topic: String,
    /// Skill level (0.0 - 1.0).
    pub level: f64,
    /// Last used date.
    pub last_used: DateTime<Utc>,
    /// Number of times used.
    pub usage_count: usize,
}

/// An earned achievement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    /// Unique achievement ID.
    pub id: String,
    /// Achievement name.
    pub name: String,
    /// Description.
    pub description: String,
    /// When earned.
    pub earned_at: DateTime<Utc>,
    /// Achievement tier.
    pub tier: AchievementTier,
}

/// Achievement tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AchievementTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

/// Streak tracking information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreakInfo {
    /// Current streak (days).
    pub current_streak: u32,
    /// Longest streak ever.
    pub longest_streak: u32,
    /// Last activity date.
    pub last_activity_date: Option<chrono::NaiveDate>,
    /// Total active days.
    pub total_active_days: u32,
}

impl StreakInfo {
    /// Record an activity.
    pub fn record_activity(&mut self, timestamp: DateTime<Utc>) {
        let date = timestamp.date_naive();
        
        if let Some(last_date) = self.last_activity_date {
            let days_diff = (date - last_date).num_days();
            
            if days_diff == 1 {
                // Consecutive day.
                self.current_streak += 1;
            } else if days_diff > 1 {
                // Streak broken.
                self.current_streak = 1;
            }
            // Same day - no change.
            
            if date != last_date {
                self.total_active_days += 1;
            }
        } else {
            // First activity.
            self.current_streak = 1;
            self.total_active_days = 1;
        }
        
        self.last_activity_date = Some(date);
        
        if self.current_streak > self.longest_streak {
            self.longest_streak = self.current_streak;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_creation() {
        let profile = UserProfile::new();
        assert_eq!(profile.total_messages, 0);
        assert_eq!(profile.experience_title(), "Intermediate");
    }

    #[test]
    fn test_completeness() {
        let profile = UserProfile::new();
        let completeness = profile.completeness();
        assert!(completeness >= 0.0 && completeness <= 1.0);
    }

    #[test]
    fn test_skill_update() {
        let mut profile = UserProfile::new();
        profile.update_skill("rust", 0.8);
        
        assert!(profile.skills.contains_key("rust"));
        assert!(profile.skills["rust"].level > 0.0);
    }

    #[test]
    fn test_streak_tracking() {
        let mut streak = StreakInfo::default();
        let now = Utc::now();
        
        streak.record_activity(now);
        assert_eq!(streak.current_streak, 1);
        assert_eq!(streak.total_active_days, 1);
    }

    #[test]
    fn test_achievements() {
        let mut profile = UserProfile::new();
        profile.total_tasks_completed = 1;
        profile.check_achievements();
        
        assert!(profile.has_achievement("first_task"));
    }
}