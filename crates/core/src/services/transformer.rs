use crate::models::subscription::SubscriptionPayload;

pub trait SubscriptionTransformer {
    fn transform(&self, payload: &SubscriptionPayload) -> SubscriptionPayload;
}

pub struct PassthroughTransformer;

impl SubscriptionTransformer for PassthroughTransformer {
    fn transform(&self, payload: &SubscriptionPayload) -> SubscriptionPayload {
        payload.clone()
    }
}